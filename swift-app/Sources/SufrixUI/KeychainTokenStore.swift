// Host secure-bytes vault backing the core's `TokenStore` (PLAN §7.2).
//
// The session blob is opaque — the core owns its meaning (token + snapshot +
// permissions); the host only persists, clears, and (at cold start) reads back
// the bytes. The same Security/Keychain API covers iOS, iPadOS and macOS.
import Foundation
import Security

final class KeychainTokenStore: TokenStore {
    private let service = "app.sufrix.session"
    private let account = "session-blob"

    private func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    /// Core → host: persist the latest session blob (insert or update).
    func saveBlob(blob: Data) {
        let attrs: [String: Any] = [kSecValueData as String: blob]
        let status = SecItemUpdate(baseQuery() as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            var add = baseQuery()
            add[kSecValueData as String] = blob
            // Available after first unlock so a background relaunch can restore.
            add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            SecItemAdd(add as CFDictionary, nil)
        }
    }

    /// Core → host: drop the blob on sign-out.
    func clearBlob() {
        SecItemDelete(baseQuery() as CFDictionary)
    }

    /// Host-only: read the blob once at launch to re-hydrate the session. Not
    /// part of the FFI trait — the core pushes writes; the host pulls this once.
    func loadBlob() -> Data? {
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess else { return nil }
        return out as? Data
    }
}
