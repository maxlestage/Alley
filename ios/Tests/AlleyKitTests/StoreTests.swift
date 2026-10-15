import Foundation
import XCTest

@testable import AlleyKit

final class StoreTests: XCTestCase {
    func vault() -> (Vault, UserDefaults) {
        let name = "alley-tests-\(UUID().uuidString)"
        let d = UserDefaults(suiteName: name)!
        return (Vault(defaults: d, deviceKey: Data(repeating: 4, count: 32)), d)
    }

    func room(_ id: String, days: Double = 1) -> RoomInfo {
        RoomInfo(id: id, name: "Salon \(id)", isPrivate: false,
                 expiresAt: Int64(Date().addingTimeInterval(days * 86_400).timeIntervalSince1970 * 1000))
    }

    func testRemembersRoomsUntilTheyEnd() {
        let (v, _) = vault()
        v.remember(room("aaaaaaaaaaaa"), server: "s", created: true)
        v.remember(room("bbbbbbbbbbbb", days: -1), server: "s", created: false)
        v.remember(room("aaaaaaaaaaaa"), server: "s", created: false)
        let rooms = v.recents()
        XCTAssertEqual(rooms.map(\.info.id), ["aaaaaaaaaaaa"])
        XCTAssertTrue(rooms[0].created, "créé un jour, créé toujours")
        v.forget("aaaaaaaaaaaa", server: "s")
        XCTAssertTrue(v.recents().isEmpty)
    }

    func testATamperedEntryIsDropped() {
        let (v, d) = vault()
        v.remember(room("aaaaaaaaaaaa"), server: "s", created: true)
        var entry = d.dictionary(forKey: "alley.rooms.v1") as! [String: String]
        entry["data"] = entry["data"]!.replacingOccurrences(of: "Salon", with: "Piège")
        d.set(entry, forKey: "alley.rooms.v1")
        XCTAssertTrue(v.recents().isEmpty)
        XCTAssertTrue(v.takeTampered())
        XCTAssertNil(d.dictionary(forKey: "alley.rooms.v1"))
    }

    func testAccountEntriesNeedTheAccount() {
        let (v, _) = vault()
        v.accountKey = Data(repeating: 5, count: 32)
        var p = Prefs()
        p.theme = .dark
        p.setMuted("abc", true)
        v.prefs = p
        XCTAssertEqual(v.prefs, p)
        v.accountKey = nil
        XCTAssertEqual(v.prefs, Prefs(), "invisible sans le compte")
        v.accountKey = Data(repeating: 6, count: 32)
        XCTAssertEqual(v.prefs, Prefs(), "un autre compte : écarté")
        XCTAssertTrue(v.takeTampered())
    }

    func testRoomsWithAForgedServerSignatureAreDropped() {
        let (v, _) = vault()
        v.remember(room("aaaaaaaaaaaa"), server: "s", created: false)
        XCTAssertTrue(v.verify(server: "s", serverKey: Brocolis.b64(Data(repeating: 1, count: 32))))
        XCTAssertTrue(v.recents().isEmpty)
    }

    func testLinks() {
        let id = "0123456789abcdef0123456789abcdef"
        XCTAssertEqual(Links.room(from: "https://alley.example/en/app/#salon=\(id)"), id)
        XCTAssertEqual(Links.room(from: " A1B2C3D4E5F6 "), "a1b2c3d4e5f6")
        XCTAssertNil(Links.room(from: "bonjour"))
        XCTAssertNil(Links.room(from: "salon=zzzzzzzzzzzz"))
        XCTAssertEqual(
            Links.invite(server: URL(string: "https://a.example/")!, room: "abc", lang: "en").absoluteString,
            "https://a.example/en/app/#salon=abc")
        XCTAssertEqual(
            Links.invite(server: URL(string: "https://a.example")!, room: "abc", lang: "fr").absoluteString,
            "https://a.example/app/#salon=abc")
    }

    func testReactionsToggle() {
        var r: [String: [String]] = [:]
        RoomSession.apply(&r, "👍", who: "a", on: true)
        RoomSession.apply(&r, "👍", who: "b", on: true)
        RoomSession.apply(&r, "👍", who: "a", on: true)
        XCTAssertEqual(r["👍"], ["b", "a"])
        RoomSession.apply(&r, "👍", who: "a", on: false)
        RoomSession.apply(&r, "👍", who: "b", on: false)
        XCTAssertNil(r["👍"])
    }
}
