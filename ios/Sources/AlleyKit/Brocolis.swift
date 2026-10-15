// Le chiffrement de la messagerie Brocolis, octet pour octet comme le
// navigateur (`client/src/crypto.rs`) : un iPhone et un navigateur se
// parlent dans le même salon.
//
// - AES-256-GCM : nonce (12 octets) ‖ chiffré ‖ étiquette (16 octets),
//   en base64 dans un jeton `[[BROCOLIS:v1:…]]` ;
// - X25519 + HKDF-SHA256 : la clé du salon est remise emballée pour un seul
//   destinataire ;
// - Ed25519 : tout ce qu'envoie un membre est signé de sa clé jetable.

import Foundation

#if canImport(CryptoKit)
import CryptoKit
#else
import Crypto
#endif

public typealias Key = Data

public enum Brocolis {
    public static let tokenStart = "[[BROCOLIS:v1:"
    public static let tokenEnd = "]]"
    static let wrapSalt = Data("brocolis-x25519-wrap".utf8)

    // MARK: Aléa et encodages

    public static func randomBytes(_ count: Int) -> Data {
        var g = SystemRandomNumberGenerator()
        return Data((0..<count).map { _ in UInt8.random(in: 0...255, using: &g) })
    }

    public static func generateKey() -> Key { randomBytes(32) }

    public static func b64(_ data: Data) -> String { data.base64EncodedString() }

    public static func unb64(_ text: String) -> Data? {
        Data(base64Encoded: text.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    /// Base64 pour les adresses : `-` et `_`, sans `=`.
    public static func b64url(_ data: Data) -> String {
        b64(data).replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    public static func hex(_ data: Data) -> String {
        data.map { String(format: "%02x", $0) }.joined()
    }

    public static func sha256(_ parts: Data...) -> Data {
        var h = SHA256()
        for p in parts { h.update(data: p) }
        return Data(h.finalize())
    }

    // MARK: AES-256-GCM

    public static func seal(_ key: Key, _ plain: Data) -> Data {
        let box = try! AES.GCM.seal(plain, using: SymmetricKey(data: key), nonce: AES.GCM.Nonce())
        return box.combined!
    }

    public static func open(_ key: Key, _ combined: Data) -> Data? {
        guard key.count == 32, combined.count >= 28,
              let box = try? AES.GCM.SealedBox(combined: combined)
        else { return nil }
        return try? AES.GCM.open(box, using: SymmetricKey(data: key))
    }

    public static func sealToken(_ key: Key, _ text: String) -> String {
        sealBytesToken(key, Data(text.utf8))
    }

    public static func sealBytesToken(_ key: Key, _ bytes: Data) -> String {
        tokenStart + b64(seal(key, bytes)) + tokenEnd
    }

    public static func openBytesToken(_ key: Key, _ token: String) -> Data? {
        guard token.hasPrefix(tokenStart), token.hasSuffix(tokenEnd) else { return nil }
        let inner = token.dropFirst(tokenStart.count).dropLast(tokenEnd.count)
        guard let raw = unb64(String(inner)) else { return nil }
        return open(key, raw)
    }

    public static func openToken(_ key: Key, _ token: String) -> String? {
        openBytesToken(key, token).flatMap { String(data: $0, encoding: .utf8) }
    }

    /// L'empreinte d'une clé de photo, que le serveur relaie avec elle.
    public static func keyCommitment(_ key: Key) -> String { hex(sha256(key)) }

    /// `2960-B214-6D42-27C9` : la même chez tous les présents.
    public static func fingerprint(_ data: Data) -> String {
        let hex = sha256(data).prefix(8).map { String(format: "%02X", $0) }.joined()
        var groups: [String] = []
        var i = hex.startIndex
        while i < hex.endIndex {
            let j = hex.index(i, offsetBy: 4)
            groups.append(String(hex[i..<j]))
            i = j
        }
        return groups.joined(separator: "-")
    }

    public static func hkdf(_ ikm: Data, salt: Data, info: Data, count: Int = 32) -> Data {
        let key = HKDF<SHA256>.deriveKey(
            inputKeyMaterial: SymmetricKey(data: ikm), salt: salt, info: info,
            outputByteCount: count)
        return key.withUnsafeBytes { Data($0) }
    }

    public static func hmac(_ key: Data, _ data: Data) -> Data {
        Data(HMAC<SHA256>.authenticationCode(for: data, using: SymmetricKey(data: key)))
    }
}

// MARK: - X25519 : remettre la clé du salon

/// Une identité X25519 jetable, le temps d'un salon.
public struct Identity {
    let secret: Curve25519.KeyAgreement.PrivateKey

    public init() { secret = Curve25519.KeyAgreement.PrivateKey() }

    public init?(raw: Data) {
        guard let s = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: raw) else { return nil }
        secret = s
    }

    public var publicRaw: Data { secret.publicKey.rawRepresentation }

    /// La clé d'emballage partagée avec `peer` (même calcul des deux côtés).
    func wrapKey(_ peerB64: String) -> Key? {
        guard let peer = Brocolis.unb64(peerB64), peer.count == 32,
              let peerKey = try? Curve25519.KeyAgreement.PublicKey(rawRepresentation: peer),
              let shared = try? secret.sharedSecretFromKeyAgreement(with: peerKey)
        else { return nil }
        let mine = publicRaw
        let info = mine.lexicographicallyPrecedes(peer) || mine == peer ? mine + peer : peer + mine
        let raw = shared.withUnsafeBytes { Data($0) }
        return Brocolis.hkdf(raw, salt: Brocolis.wrapSalt, info: info)
    }

    public func wrapGroupKey(_ key: Key, for peerB64: String) -> String? {
        wrapKey(peerB64).map { Brocolis.b64(Brocolis.seal($0, key)) }
    }

    public func unwrapGroupKey(_ blobB64: String, from peerB64: String) -> Key? {
        guard let wrap = wrapKey(peerB64), let blob = Brocolis.unb64(blobB64),
              let key = Brocolis.open(wrap, blob), key.count == 32
        else { return nil }
        return key
    }
}

// MARK: - Ed25519 : tout est signé

/// La clé qui signe tout ce que j'envoie dans un salon (jetable).
public struct Signer {
    let key: Curve25519.Signing.PrivateKey

    public init() { key = Curve25519.Signing.PrivateKey() }

    public init?(raw: Data) {
        guard let k = try? Curve25519.Signing.PrivateKey(rawRepresentation: raw) else { return nil }
        key = k
    }

    public var publicRaw: Data { key.publicKey.rawRepresentation }

    func sign(_ data: Data) -> Data { (try? key.signature(for: data)) ?? Data() }
}

/// Pourquoi un envoi est écarté.
public enum Rejected: Error, Equatable {
    case unreadable, unsigned, forged, replayed, misdirected
}

/// Les numéros d'envoi déjà vus d'un membre : un envoi rejoué est écarté.
public struct Replay: Sendable {
    public static let window: UInt64 = 4096
    var max: UInt64 = 0
    var seen = Set<UInt64>()

    public init() {}

    public func fresh(_ n: UInt64) -> Bool {
        n > 0 && n + Replay.window > max && !seen.contains(n)
    }

    public mutating func mark(_ n: UInt64) {
        seen.insert(n)
        max = Swift.max(max, n)
        let floor = max > Replay.window ? max - Replay.window : 0
        seen = seen.filter { $0 >= floor }
    }
}

public enum Envelope {
    static let domain = Data("alley-sig-v1".utf8)

    static func signedBytes(room: String, to: String, n: UInt64, body: String) -> Data {
        var out = Data()
        for part in [domain, Data(room.utf8), Data(to.utf8)] {
            out.append(part)
            out.append(0)
        }
        withUnsafeBytes(of: n.bigEndian) { out.append(contentsOf: $0) }
        out.append(Data(body.utf8))
        return out
    }

    /// Chiffre et signe un envoi (pour `to` seul, ou pour tout le salon).
    public static func seal(
        key: Key, signer: Signer, room: String, to: String?, n: UInt64, body: String
    ) -> String {
        let to = to ?? ""
        let sig = signer.sign(signedBytes(room: room, to: to, n: n, body: body))
        let envelope: [String: Any] = ["b": body, "to": to, "n": n, "sig": Brocolis.b64(sig)]
        let json = try! JSONSerialization.data(withJSONObject: envelope)
        return Brocolis.sealToken(key, String(data: json, encoding: .utf8)!)
    }

    /// Ouvre un envoi : seulement s'il est signé de la clé de son auteur,
    /// pour ce salon et pour moi, et jamais vu.
    public static func open(
        key: Key, token: String, signer: String?, room: String, me: String, seen: Replay
    ) -> Result<(String, UInt64), Rejected> {
        guard let clear = Brocolis.openToken(key, token),
              let envelope = (try? JSONSerialization.jsonObject(with: Data(clear.utf8))) as? [String: Any]
        else { return .failure(.unreadable) }
        guard let s = signer, let raw = Brocolis.unb64(s), raw.count == 32,
              let pub = try? Curve25519.Signing.PublicKey(rawRepresentation: raw)
        else { return .failure(.unsigned) }
        guard let body = envelope["b"] as? String, let to = envelope["to"] as? String,
              let n = (envelope["n"] as? NSNumber)?.uint64Value,
              let sigText = envelope["sig"] as? String, let sig = Brocolis.unb64(sigText), sig.count == 64
        else { return .failure(.forged) }
        guard pub.isValidSignature(sig, for: signedBytes(room: room, to: to, n: n, body: body)) else {
            return .failure(.forged)
        }
        if !to.isEmpty && to != me { return .failure(.misdirected) }
        if !seen.fresh(n) { return .failure(.replayed) }
        return .success((body, n))
    }
}

// MARK: - Ce que dit le serveur des salons

public enum ServerSignature {
    /// Ce que le serveur signe d'un salon : identifiant, nom, type, fin.
    public static func roomStatement(id: String, name: String, isPrivate: Bool, expiresAt: Int64) -> Data {
        var out = Data("alley-room-v1".utf8)
        out.append(0)
        for part in [id, name, isPrivate ? "1" : "0"] {
            out.append(Data(part.utf8))
            out.append(0)
        }
        out.append(Data(String(expiresAt).utf8))
        return out
    }

    public static func verify(
        serverKey: String, id: String, name: String, isPrivate: Bool, expiresAt: Int64, sig: String
    ) -> Bool {
        guard let raw = Brocolis.unb64(serverKey), raw.count == 32,
              let pub = try? Curve25519.Signing.PublicKey(rawRepresentation: raw),
              let s = Brocolis.unb64(sig), s.count == 64
        else { return false }
        return pub.isValidSignature(
            s, for: roomStatement(id: id, name: name, isPrivate: isPrivate, expiresAt: expiresAt))
    }
}
