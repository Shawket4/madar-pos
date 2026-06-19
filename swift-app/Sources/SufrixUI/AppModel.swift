// The host's single source of UI state. Owns the one `SufrixCore` handle and
// the secure vault, mirrors the core's session into `@Published` state, and
// forwards sign-in/out. NO business logic — the online↔offline decision, token
// custody, and validation all live in the core (`sign_in`).
import Foundation
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    let core: SufrixCore
    private let vault = KeychainTokenStore()

    /// The active session, or `nil` when signed out. Drives the root route.
    @Published private(set) var session: SessionSnapshot?
    @Published private(set) var isBusy = false
    @Published var errorMessage: String?

    /// The device's configured branch — PIN login derives the org from it
    /// (post-D13 any active org teller may sign in here). Set once at device
    /// provisioning; persisted locally.
    @Published var branchId: String {
        didSet { UserDefaults.standard.set(branchId, forKey: Self.branchKey) }
    }

    init() {
        var cfg = defaultConfig()
        cfg.dbPath = Self.databasePath()
        cfg.locale = Locale.current.identifier
        // A failed store open is unrecoverable — fail loudly rather than limp on.
        core = try! SufrixCore(config: cfg)
        branchId = UserDefaults.standard.string(forKey: Self.branchKey) ?? ""

        core.setTokenStore(store: vault)
        // Cold-start: re-hydrate the last session from the Keychain blob.
        if let blob = vault.loadBlob() {
            session = core.restoreSession(blob: blob)
        }
    }

    var isSignedIn: Bool { session != nil }

    // ── intents ─────────────────────────────────────────────────────────────

    func signInTeller(name: String, pin: String) async {
        await run {
            try await self.core.signIn(req: LoginRequest(
                mode: .pin, name: name, pin: pin, branchId: self.branchId,
                email: nil, password: nil, orgId: nil))
        }
    }

    func signInManager(email: String, password: String) async {
        await run {
            try await self.core.signIn(req: LoginRequest(
                mode: .email, name: nil, pin: nil, branchId: nil,
                email: email, password: password, orgId: nil))
        }
    }

    func signOut() {
        try? core.logout(wipeOutbox: false)
        session = nil
        errorMessage = nil
    }

    // ── plumbing ────────────────────────────────────────────────────────────

    private func run(_ op: @escaping () async throws -> SessionSnapshot) async {
        isBusy = true
        errorMessage = nil
        defer { isBusy = false }
        do {
            session = try await op()
        } catch {
            errorMessage = Self.humanMessage(error)
        }
    }

    /// Map the coarse `CoreError` to something a teller can read.
    static func humanMessage(_ error: Error) -> String {
        guard let e = error as? CoreError else { return error.localizedDescription }
        switch e {
        case .Offline:
            return "You're offline and this teller hasn't been set up for offline sign-in yet."
        case let .Unauthenticated(message):
            return message
        case let .Validation(_, message):
            return message
        case let .Server(_, _, message):
            return message
        case let .Transient(message):
            return "Network problem: \(message)"
        case let .Forbidden(resource, action):
            return "Not allowed: \(resource)/\(action)"
        case let .Internal(message):
            return "Something went wrong: \(message)"
        }
    }

    private static let branchKey = "sufrix.branch_id"

    /// App-private SQLite path under Application Support.
    private static func databasePath() -> String {
        let fm = FileManager.default
        let dir = (try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask,
                               appropriateFor: nil, create: true))
            ?? fm.temporaryDirectory
        return dir.appendingPathComponent("sufrix.sqlite").path
    }
}
