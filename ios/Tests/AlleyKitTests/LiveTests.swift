import Foundation
import XCTest

@testable import AlleyKit

/// Contre un vrai serveur Alley (`ALLEY_SERVER=http://localhost:3999`) :
/// deux iPhones dans un salon, et, avec `ALLEY_INTEROP_ROOM`, un iPhone qui
/// parle à un navigateur. Sans serveur, ces tests ne font rien.
/// La WebSocket, par un pont Node (sous Linux, URLSession n'en a pas).
final class BridgeTransport: RoomTransport, @unchecked Sendable {
    let process = Process()
    let input = Pipe()
    let output = Pipe()
    var buffer = Data()

    @MainActor
    init(url: URL, session: RoomSession) {
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        let script = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("bridge.mjs").path
        process.arguments = ["node", script, url.absoluteString]
        process.standardInput = input
        process.standardOutput = output
        output.fileHandleForReading.readabilityHandler = { [weak self, weak session] h in
            guard let self else { return }
            self.buffer.append(h.availableData)
            while let i = self.buffer.firstIndex(of: 10) {
                let line = String(decoding: self.buffer[self.buffer.startIndex..<i], as: UTF8.self)
                self.buffer.removeSubrange(self.buffer.startIndex...i)
                Task { @MainActor in
                    if line.hasPrefix("CLOSE ") {
                        session?.lost(code: Int(line.dropFirst(6)) ?? 1006)
                    } else if let text = try? JSONDecoder().decode(String.self, from: Data(line.utf8)) {
                        session?.receive(text)
                    }
                }
            }
        }
        try! process.run()
    }

    func send(_ text: String) {
        let line = String(decoding: try! JSONEncoder().encode(text), as: UTF8.self) + "\n"
        input.fileHandleForWriting.write(Data(line.utf8))
    }

    func close() {
        input.fileHandleForWriting.write(Data("CLOSE\n".utf8))
    }
}

@MainActor
final class LiveTests: XCTestCase {
    var api: API!

    override func setUp() async throws {
        guard let base = ProcessInfo.processInfo.environment["ALLEY_SERVER"] else {
            throw XCTSkip("ALLEY_SERVER non défini")
        }
        api = API(base: URL(string: base)!)
    }

    func wait(_ what: String, timeout: TimeInterval = 15, _ ok: @escaping () -> Bool) async throws {
        let end = Date().addingTimeInterval(timeout)
        while !ok() {
            if Date() > end {
                XCTFail("délai dépassé : \(what)")
                throw CancellationError()
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
    }

    func session(_ info: RoomInfo, _ name: String, _ account: Account? = nil) -> RoomSession {
        let s = RoomSession(info: info, api: api, pseudonym: name, account: account)
        #if os(Linux)
        s.makeTransport = { BridgeTransport(url: $0, session: $1) }
        #endif
        return s
    }

    func testTwoPhonesTalkInARoom() async throws {
        let room = try await api.create(name: "iPhone \(Int.random(in: 1000...9999))", isPrivate: true, account: nil)
        let serverKey = try await api.serverKey()
        XCTAssertTrue(room.signed(by: serverKey), "le serveur signe le salon")
        let ada = session(room, "Ada", Account(pseudonym: "Ada", seed: Data(repeating: 9, count: 32)))
        ada.connect()
        try await wait("clé d'Ada") { ada.state == .ready }
        XCTAssertTrue(ada.iAmAdmin)
        let bob = session(room, "Bob")
        bob.connect()
        try await wait("clé remise à Bob") { bob.state == .ready }
        XCTAssertEqual(ada.fingerprint, bob.fingerprint)
        try await wait("noms échangés") { bob.name(of: ada.me) == "Ada" && ada.name(of: bob.me) == "Bob" }

        XCTAssertTrue(ada.sendText("Bonjour Bob 👋"))
        try await wait("message reçu") { bob.messages.contains { $0.text == "Bonjour Bob 👋" && $0.name == "Ada" } }
        let mid = bob.messages.last!.id
        bob.react(mid, "❤️")
        try await wait("réaction") { ada.messages.first?.reactions["❤️"] == [bob.me] }
        ada.edit(mid, text: "Bonjour Bob !")
        try await wait("modifié") { bob.messages.last?.text == "Bonjour Bob !" && bob.messages.last?.edited == true }
        XCTAssertFalse(bob.sendText("de la cocaïne ?"), "le filtre refuse")
        XCTAssertEqual(bob.notice, .refused(.drugs))
        bob.sendText("Salut Ada", reply: mid)
        try await wait("réponse") { ada.messages.last?.reply == mid }

        // Une photo, vérifiée par le serveur avant d'être relayée.
        let jpeg = Data([0xFF, 0xD8, 0xFF, 0xE0]) + Data(repeating: 0, count: 64)
        bob.sendImage(jpeg)
        try await wait("photo jugée", timeout: 20) { bob.messages.last?.pending == false || bob.notice == .photoRefused("invalid") || bob.notice == .photoRefused("unavailable") }

        // L'administratrice renomme, puis supprime le message de Bob.
        XCTAssertFalse(bob.rename("Pirate"))
        XCTAssertTrue(ada.rename("Salon renommé"))
        try await wait("renommé") { bob.info.name == "Salon renommé" }
        XCTAssertEqual(bob.notice, .renamed("Salon renommé", byMe: false))
        XCTAssertTrue(bob.info.signed(by: serverKey), "le nouveau nom est signé")
        let bobs = ada.messages.first { $0.from == bob.me && $0.kind == .text }!.id
        ada.delete(bobs)
        try await wait("supprimé chez Bob") { !bob.messages.contains { $0.id == bobs } }

        ada.expel(bob.me)
        try await wait("Bob exclu") { bob.state == .closed(.expelled) }
        try await wait("Bob parti") { ada.peers.isEmpty }
        ada.leave()
    }

    /// Avec un navigateur dans le salon `ALLEY_INTEROP_ROOM` (voir
    /// `ios/interop.mjs`).
    func testTalksWithTheBrowser() async throws {
        guard let id = ProcessInfo.processInfo.environment["ALLEY_INTEROP_ROOM"] else {
            throw XCTSkip("ALLEY_INTEROP_ROOM non défini")
        }
        let info = try await api.room(id)
        let me = session(info, "iPhone")
        me.connect()
        try await wait("clé remise par le navigateur", timeout: 20) { me.state == .ready }
        try await wait("message du navigateur", timeout: 30) { me.messages.contains { $0.text == "Bonjour depuis le navigateur" } }
        let from = me.messages.first { $0.text == "Bonjour depuis le navigateur" }!
        XCTAssertEqual(from.name, "Navigateur")
        me.react(from.id, "👍")
        me.sendText("Bonjour depuis l'iPhone 📱", reply: from.id)
        try await wait("réaction du navigateur", timeout: 30) { me.messages.contains { $0.text == "Bien reçu" } }
        print("INTEROP-FINGERPRINT \(me.fingerprint)")
        try await Task.sleep(nanoseconds: 1_000_000_000)
        me.leave()
    }
}
