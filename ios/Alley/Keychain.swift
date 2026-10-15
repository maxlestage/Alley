// La clé de l'appareil, qui signe ce que garde Alley (salons, réglages) :
// tirée au hasard une fois, gardée dans le trousseau de l'iPhone, jamais
// sauvegardée ailleurs ni synchronisée.

import AlleyKit
import Foundation

#if canImport(Security)
import Security
#endif

enum Keychain {
    static let account = "alley.device-key.v1"

    static func deviceKey() -> Data {
        #if canImport(Security)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        if SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess, let data = item as? Data, data.count == 32 {
            return data
        }
        let key = Brocolis.randomBytes(32)
        let add: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: account,
            kSecValueData as String: key,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        SecItemDelete(add as CFDictionary)
        SecItemAdd(add as CFDictionary, nil)
        return key
        #else
        return Data(repeating: 0, count: 32)
        #endif
    }
}
