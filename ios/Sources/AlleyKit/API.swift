// L'API du serveur Alley : la liste des salons, en créer un, le retrouver,
// les salons du compte, ceux que l'appareil connaît (rouverts si le serveur
// les a oubliés), la clé publique du serveur.

import Foundation

#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Un salon, tel que le serveur le décrit (et le signe).
public struct RoomInfo: Codable, Hashable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var isPrivate: Bool
    /// Sa fin, en millisecondes depuis 1970.
    public var expiresAt: Int64
    public var people: Int
    public var sig: String

    enum CodingKeys: String, CodingKey {
        case id, name, sig
        case people = "members"
        case isPrivate = "private"
        case expiresAt = "expires_at"
    }

    public init(id: String, name: String, isPrivate: Bool, expiresAt: Int64, people: Int = 0, sig: String = "") {
        self.id = id
        self.name = name
        self.isPrivate = isPrivate
        self.expiresAt = expiresAt
        self.people = people
        self.sig = sig
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = (try? c.decode(String.self, forKey: .name)) ?? "Alley"
        isPrivate = (try? c.decode(Bool.self, forKey: .isPrivate)) ?? false
        expiresAt = (try? c.decode(Int64.self, forKey: .expiresAt))
            ?? Int64((try? c.decode(Double.self, forKey: .expiresAt)) ?? 0)
        people = (try? c.decode(Int.self, forKey: .people)) ?? 0
        sig = (try? c.decode(String.self, forKey: .sig)) ?? ""
    }

    public var expires: Date { Date(timeIntervalSince1970: Double(expiresAt) / 1000) }

    /// Ce que le serveur en a dit, avec sa signature : de quoi le rouvrir.
    public var statement: [String: Any] {
        ["id": id, "name": name, "private": isPrivate, "expires_at": expiresAt, "sig": sig]
    }

    public func signed(by serverKey: String) -> Bool {
        ServerSignature.verify(
            serverKey: serverKey, id: id, name: name, isPrivate: isPrivate, expiresAt: expiresAt, sig: sig)
    }
}

/// Une erreur de l'API : le code renvoyé (`banned`, `bad_name`…), ou le réseau.
public struct APIError: Error, Equatable, Sendable {
    public let status: Int
    public let code: String

    public var banned: Bool { code == "banned" }
    public var network: Bool { status == 0 }
}

public final class API: @unchecked Sendable {
    public let base: URL
    let session: URLSession

    public init(base: URL, session: URLSession = .shared) {
        self.base = base
        self.session = session
    }

    /// L'adresse WebSocket du serveur (`wss://…`).
    public var socketBase: String {
        let s = base.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        if s.hasPrefix("https://") { return "wss://" + s.dropFirst(8) }
        if s.hasPrefix("http://") { return "ws://" + s.dropFirst(7) }
        return s
    }

    func request(_ method: String, _ path: String, body: [String: Any]? = nil) async throws -> Any {
        var req = URLRequest(url: URL(string: base.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + path)!)
        req.httpMethod = method
        req.cachePolicy = .reloadIgnoringLocalCacheData
        req.timeoutInterval = 20
        if let body {
            req.setValue("application/json", forHTTPHeaderField: "content-type")
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: req)
        } catch {
            throw APIError(status: 0, code: "network")
        }
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        let json = data.isEmpty ? nil : try? JSONSerialization.jsonObject(with: data)
        guard (200..<300).contains(status) else {
            let obj = json as? [String: Any]
            let code = (obj?["error"] as? String) ?? (obj?["code"] as? String) ?? ""
            throw APIError(status: status, code: code)
        }
        return json ?? NSNull()
    }

    func decode<T: Decodable>(_ type: T.Type, _ value: Any) throws -> T {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        return try JSONDecoder().decode(T.self, from: data)
    }

    func rooms(in value: Any) -> [RoomInfo] {
        let list = ((value as? [String: Any])?["rooms"] as? [Any]) ?? []
        return list.compactMap { try? decode(RoomInfo.self, $0) }
    }

    /// Les salons publics ouverts (filtrés par `query`).
    public func list(query: String = "") async throws -> [RoomInfo] {
        let q = query.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed.subtracting(["&", "=", "+", "#"])) ?? ""
        return rooms(in: try await request("GET", "/api/v1/rooms?q=\(q)"))
    }

    /// Crée un salon (ou rejoint le salon public de ce nom). Avec un compte,
    /// une période payée en cours le fait durer jusqu'à sa fin.
    public func create(name: String, isPrivate: Bool, account: Account?, payment: String? = nil) async throws -> RoomInfo {
        var body: [String: Any] = [
            "name": name, "private": isPrivate, "tz": TimeZone.current.secondsFromGMT() / 60,
        ]
        if let account { body["billing"] = account.billing }
        if let payment { body["payment"] = payment }
        return try decode(RoomInfo.self, try await request("POST", "/api/v1/rooms", body: body))
    }

    public func room(_ id: String) async throws -> RoomInfo {
        try decode(RoomInfo.self, try await request("GET", "/api/v1/rooms/\(id)"))
    }

    /// Les salons ouverts du compte, retrouvés grâce à sa marque.
    public func mine(_ account: Account) async throws -> [RoomInfo] {
        rooms(in: try await request("POST", "/api/v1/mine", body: ["tag": account.tag]))
    }

    /// Ceux que l'appareil connaît encore ouverts ; rouverts s'ils ont été
    /// oubliés (le serveur reconnaît sa signature).
    public func known(_ rooms: [RoomInfo]) async throws -> [RoomInfo] {
        let body: [String: Any] = [
            "ids": rooms.map(\.id), "rooms": rooms.filter { !$0.sig.isEmpty }.map(\.statement),
        ]
        return self.rooms(in: try await request("POST", "/api/v1/known", body: body))
    }

    /// La clé publique (Ed25519, base64) qui signe ce que dit le serveur.
    public func serverKey() async throws -> String {
        let v = try await request("GET", "/api/v1/key")
        guard let key = (v as? [String: Any])?["key"] as? String else {
            throw APIError(status: 500, code: "key")
        }
        return key
    }

    /// Les serveurs ICE des appels (STUN, et TURN s'il y en a) :
    /// `[{"urls": [...], "username"?, "credential"?}]`.
    public func iceServers() async -> [[String: Any]] {
        let fallback: [[String: Any]] = [["urls": ["stun:stun.cloudflare.com:3478", "stun:stun.l.google.com:19302"]]]
        guard let v = try? await request("GET", "/api/v1/ice") as? [String: Any],
              let list = v["iceServers"] as? [[String: Any]], !list.isEmpty
        else { return fallback }
        return list
    }

    /// La période payée du compte (`payment` : `apple:<transaction>` pour un
    /// achat sur iPhone) : sa fin, si elle court encore.
    public func periodEnd(_ account: Account, payment: String?) async -> Date? {
        var body: [String: Any] = ["secret": account.billing, "tz": TimeZone.current.secondsFromGMT() / 60]
        if let payment { body["payment"] = payment }
        guard let v = try? await request("POST", "/api/v1/billing/status", body: body) as? [String: Any],
              v["active"] as? Bool == true, let until = (v["until"] as? NSNumber)?.doubleValue
        else { return nil }
        return Date(timeIntervalSince1970: until / 1000)
    }

    /// Une requête anodine : refusée si cette adresse est bloquée.
    public func stats() async throws -> [String: Any] {
        (try await request("GET", "/api/v1/stats") as? [String: Any]) ?? [:]
    }
}
