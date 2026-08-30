import Foundation
import Security

struct ConnectionSettings: Equatable {
    var backendURL: String
    var token: String

    var isComplete: Bool {
        guard
            let url = URL(string: backendURL),
            url.scheme == "wss",
            url.host != nil
        else {
            return false
        }
        return token.utf8.count >= 32 && token.utf8.count <= 256
    }
}

enum SecureStoreError: LocalizedError {
    case unexpectedStatus(OSStatus)
    case invalidText

    var errorDescription: String? {
        switch self {
        case let .unexpectedStatus(status):
            "Keychain operation failed with status \(status)."
        case .invalidText:
            "The Keychain value is not valid text."
        }
    }
}

enum SecureStore {
    private static let service = "com.alexjercan.scufris"

    static func read(_ account: String) throws -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess else {
            throw SecureStoreError.unexpectedStatus(status)
        }
        guard
            let data = result as? Data,
            let value = String(data: data, encoding: .utf8)
        else {
            throw SecureStoreError.invalidText
        }
        return value
    }

    static func write(_ value: String, account: String) throws {
        let data = Data(value.utf8)
        let identity: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let update: [String: Any] = [kSecValueData as String: data]
        let status = SecItemUpdate(identity as CFDictionary, update as CFDictionary)
        if status == errSecSuccess {
            return
        }
        guard status == errSecItemNotFound else {
            throw SecureStoreError.unexpectedStatus(status)
        }
        var addition = identity
        addition[kSecValueData as String] = data
        addition[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let addStatus = SecItemAdd(addition as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw SecureStoreError.unexpectedStatus(addStatus)
        }
    }
}
