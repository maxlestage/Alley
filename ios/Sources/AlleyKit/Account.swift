// Un compte : pseudonyme + mot de passe. Rien n'est envoyé au serveur
// qu'une marque (`tag`) tirée du mot de passe, et le secret de paiement :
// les mêmes que dans le navigateur, pour retrouver ses salons partout.

import Foundation

#if canImport(CommonCrypto)
import CommonCrypto
#endif

public struct Account: Equatable, Sendable {
    public let pseudonym: String
    /// La marque du compte : le serveur y rattache ses salons.
    public let tag: String
    /// Le secret de paiement (dont le serveur ne garde que l'empreinte).
    public let billing: String
    /// La clé qui signe ce que l'appareil garde pour ce compte.
    public let vaultKey: Data

    public static let iterations: UInt32 = 310_000

    public static func salt(_ pseudonym: String) -> Data {
        Data("alley:v1:\(pseudonym.trimmingCharacters(in: .whitespacesAndNewlines).lowercased())".utf8)
    }

    /// La graine du compte : PBKDF2-SHA256 (310 000 tours) du mot de passe.
    public static func seed(pseudonym: String, password: String, iterations: UInt32 = iterations) -> Data {
        pbkdf2(password: Data(password.utf8), salt: salt(pseudonym), iterations: iterations)
    }

    /// Un compte dont on ne connaît que le secret de paiement (tests).
    public init(pseudonym: String, tag: String, billing: String) {
        self.pseudonym = pseudonym
        self.tag = tag
        self.billing = billing
        vaultKey = Data(count: 32)
    }

    public init(pseudonym: String, seed: Data) {
        self.pseudonym = pseudonym
        let salt = Data("alley-account".utf8)
        tag = Brocolis.hex(Brocolis.hkdf(seed, salt: salt, info: Data("rooms-tag".utf8)))
        billing = Brocolis.hex(Brocolis.hkdf(seed, salt: salt, info: Data("billing".utf8)))
        vaultKey = Brocolis.hkdf(seed, salt: salt, info: Data("vault".utf8))
    }

    public init(pseudonym: String, password: String) {
        self.init(pseudonym: pseudonym, seed: Account.seed(pseudonym: pseudonym, password: password))
    }

    /// L'identifiant de paiement du compte : l'empreinte de son secret (le
    /// serveur ne connaît que lui).
    public static func billingID(_ secret: String) -> String {
        Brocolis.hex(Brocolis.sha256(Data("alley-billing-v1:\(secret)".utf8)))
    }

    /// L'`appAccountToken` des achats sur iPhone : un UUID tiré de
    /// l'identifiant de paiement (le serveur fait le même calcul et n'ouvre
    /// une période qu'au compte qui l'a achetée).
    public var appleToken: UUID {
        var b = [UInt8](Brocolis.sha256(Data("alley-apple-v1:\(Account.billingID(billing))".utf8)).prefix(16))
        b[6] = (b[6] & 0x0f) | 0x40
        b[8] = (b[8] & 0x3f) | 0x80
        return UUID(uuid: (b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]))
    }

    static func pbkdf2(password: Data, salt: Data, iterations: UInt32) -> Data {
        #if canImport(CommonCrypto)
        var out = Data(count: 32)
        let status = out.withUnsafeMutableBytes { o in
            password.withUnsafeBytes { p in
                salt.withUnsafeBytes { s in
                    CCKeyDerivationPBKDF(
                        CCPBKDFAlgorithm(kCCPBKDF2),
                        p.baseAddress?.assumingMemoryBound(to: Int8.self), password.count,
                        s.baseAddress?.assumingMemoryBound(to: UInt8.self), salt.count,
                        CCPseudoRandomAlgorithm(kCCPRFHmacAlgSHA256), iterations,
                        o.baseAddress?.assumingMemoryBound(to: UInt8.self), 32)
                }
            }
        }
        precondition(status == kCCSuccess, "PBKDF2")
        return out
        #else
        // Un seul bloc de 32 octets : U1 = HMAC(P, S ‖ 1), Ui = HMAC(P, Ui-1).
        var u = Brocolis.hmac(password, salt + Data([0, 0, 0, 1]))
        var out = [UInt8](u)
        if iterations > 1 {
            for _ in 1..<iterations {
                u = Brocolis.hmac(password, u)
                for (i, b) in u.enumerated() { out[i] ^= b }
            }
        }
        return Data(out)
        #endif
    }
}
