// Ce que garde l'iPhone : la liste des salons créés ou visités (jusqu'à
// leur fin) et les réglages. Jamais un message, une clé ou un pseudonyme.
// Chaque entrée est signée (HMAC-SHA256) par la clé de l'appareil, gardée
// dans le trousseau, et par celle du compte connecté : une donnée modifiée
// hors d'Alley est écartée. Chaque salon garde aussi la signature du
// serveur sur ce qu'il en a dit.

import Foundation

/// Un salon retenu.
public struct RecentRoom: Codable, Hashable, Identifiable, Sendable {
    public var info: RoomInfo
    /// Le serveur du salon.
    public var server: String
    public var created: Bool
    public var seen: Date

    public var id: String { "\(server)|\(info.id)" }
}

/// Les réglages de l'appareil.
public struct Prefs: Codable, Equatable, Sendable {
    public enum Theme: String, Codable, CaseIterable, Sendable { case auto, light, dark }

    public var theme: Theme = .auto
    public var notify = true
    public var sound = true
    /// Les salons en sourdine.
    public var muted: [String] = []
    /// La langue choisie (`fr`, `en`, `es`), sinon celle de l'iPhone.
    public var lang: String?

    public init() {}

    public func isMuted(_ room: String) -> Bool { muted.contains(room) }

    public mutating func setMuted(_ room: String, _ on: Bool) {
        muted.removeAll { $0 == room }
        if on { muted.append(room) }
        if muted.count > 50 { muted.removeFirst(muted.count - 50) }
    }
}

/// Un coffre signé, posé sur `UserDefaults`.
public final class Vault: @unchecked Sendable {
    let defaults: UserDefaults
    let deviceKey: Data
    /// La clé du compte connecté : une entrée écrite connecté doit en porter
    /// la signature.
    public var accountKey: Data?
    /// Une entrée a été écartée (modifiée hors d'Alley).
    public private(set) var tampered = false

    public init(defaults: UserDefaults = .standard, deviceKey: Data) {
        self.defaults = defaults
        self.deviceKey = deviceKey
    }

    static func statement(_ name: String, _ data: String) -> Data {
        var out = Data("alley-vault-v1".utf8)
        out.append(0)
        out.append(Data(name.utf8))
        out.append(0)
        out.append(Data(data.utf8))
        return out
    }

    public func write(_ name: String, _ data: String) {
        let s = Vault.statement(name, data)
        var entry = ["v": "1", "data": data, "dev": Brocolis.b64(Brocolis.hmac(deviceKey, s))]
        if let accountKey { entry["acc"] = Brocolis.b64(Brocolis.hmac(accountKey, s)) }
        defaults.set(entry, forKey: "alley.\(name)")
    }

    /// L'entrée, si elle est intacte ; sinon elle est retirée.
    public func read(_ name: String) -> String? {
        guard let entry = defaults.dictionary(forKey: "alley.\(name)") as? [String: String] else { return nil }
        guard let data = entry["data"] else { return reject(name) }
        let s = Vault.statement(name, data)
        guard entry["dev"] == Brocolis.b64(Brocolis.hmac(deviceKey, s)) else { return reject(name) }
        if let acc = entry["acc"] {
            // Écrite connecté : lisible seulement par ce compte.
            guard let accountKey else { return nil }
            guard acc == Brocolis.b64(Brocolis.hmac(accountKey, s)) else { return reject(name) }
        }
        return data
    }

    func reject(_ name: String) -> String? {
        tampered = true
        defaults.removeObject(forKey: "alley.\(name)")
        return nil
    }

    public func remove(_ name: String) { defaults.removeObject(forKey: "alley.\(name)") }

    public func takeTampered() -> Bool {
        defer { tampered = false }
        return tampered
    }

    // MARK: Les salons et les réglages

    static let max = 30

    public func recents(now: Date = Date()) -> [RecentRoom] {
        guard let raw = read("rooms.v1"),
              let rooms = try? JSONDecoder().decode([RecentRoom].self, from: Data(raw.utf8))
        else { return [] }
        return Vault.alive(rooms, now: now)
    }

    static func alive(_ rooms: [RecentRoom], now: Date) -> [RecentRoom] {
        Array(rooms.filter { $0.info.expires > now && !$0.info.id.isEmpty }
            .sorted { $0.seen > $1.seen }.prefix(max))
    }

    public func saveRecents(_ rooms: [RecentRoom]) {
        let rooms = Vault.alive(rooms, now: Date())
        if let data = try? JSONEncoder().encode(rooms) { write("rooms.v1", String(decoding: data, as: UTF8.self)) }
    }

    /// Ajoute (ou rafraîchit) un salon, d'après ce qu'en dit le serveur.
    public func remember(_ info: RoomInfo, server: String, created: Bool) {
        var rooms = recents()
        let previous = rooms.first { $0.info.id == info.id && $0.server == server }
        rooms.removeAll { $0.info.id == info.id && $0.server == server }
        rooms.append(RecentRoom(info: info, server: server, created: created || previous?.created == true, seen: Date()))
        saveRecents(rooms)
    }

    public func forget(_ id: String, server: String) {
        saveRecents(recents().filter { !($0.info.id == id && $0.server == server) })
    }

    public func forgetAll() { remove("rooms.v1") }

    /// Écarte les salons de ce serveur dont la signature est fausse. Vrai si
    /// l'un l'a été.
    @discardableResult
    public func verify(server: String, serverKey: String) -> Bool {
        let rooms = recents()
        let kept = rooms.filter { $0.server != server || $0.info.signed(by: serverKey) }
        if kept.count != rooms.count {
            tampered = true
            saveRecents(kept)
            return true
        }
        return false
    }

    public var prefs: Prefs {
        get {
            read("prefs.v1").flatMap { try? JSONDecoder().decode(Prefs.self, from: Data($0.utf8)) } ?? Prefs()
        }
        set {
            if let data = try? JSONEncoder().encode(newValue) { write("prefs.v1", String(decoding: data, as: UTF8.self)) }
        }
    }
}

/// Les liens de salon : `https://…/app/#salon=<id>`.
public enum Links {
    /// L'identifiant de salon d'un lien d'invitation, ou d'un identifiant
    /// collé tel quel ; `nil` si ce n'en est pas un.
    public static func room(from text: String) -> String? {
        var t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let r = t.range(of: "salon=", options: .backwards) { t = String(t[r.upperBound...]) }
        let id = String(t.prefix { $0.isASCII && ($0.isLetter || $0.isNumber) }).lowercased()
        guard [12, 32].contains(id.count), id.allSatisfy(\.isHexDigit) else { return nil }
        return id
    }

    /// Le lien d'invitation d'un salon (dans la langue choisie).
    public static func invite(server: URL, room: String, lang: String) -> URL {
        let prefix = lang == "fr" ? "" : "/\(lang)"
        let base = server.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        return URL(string: "\(base)\(prefix)/app/#salon=\(room)")!
    }
}
