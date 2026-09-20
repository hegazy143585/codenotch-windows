import Foundation

/// The Ollama cloud API key, read from the environment first and the keychain
/// second.
///
/// Unlike every other provider, Codenotch owns this credential: the user types
/// it into Settings, and it is stored in the login keychain under a service
/// name no other app uses. The environment variable `OLLAMA_API_KEY` is checked
/// first, so a shell that already exports one works without any setup.
enum OllamaCredentials {
    static let keychainService = "ollama-api-key"
    static let keychainAccount = "codenotch"

    /// The API key, wherever it is found. Environment first, then keychain.
    static func load() -> String? {
        if let env = ProcessInfo.processInfo.environment["OLLAMA_API_KEY"],
           !env.isEmpty {
            return env
        }
        return KeychainItem.read(service: keychainService, account: keychainAccount)
    }

    /// Whether a key is available from either source.
    static var isPresent: Bool { load() != nil }

    /// Stores a key in the keychain, so it survives relaunches. Overwrites any
    /// existing item under the same service+account.
    @discardableResult
    static func store(_ key: String) -> Bool {
        KeychainItem.store(service: keychainService, account: keychainAccount, value: key)
    }

    /// Removes the key from the keychain. Called on sign-out, so the next
    /// fetch finds nothing and the ring goes dark.
    @discardableResult
    static func delete() -> Bool {
        KeychainItem.delete(service: keychainService, account: keychainAccount)
    }
}
