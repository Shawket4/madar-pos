// The host's single source of UI state. Owns the one `SufrixCore` handle and
// the secure vault, mirrors the core's session into `@Published` state, and
// forwards sign-in/out. NO business logic — the online↔offline decision, token
// custody, and validation all live in the core.
import CoreText
import Foundation
import SwiftUI

/// Device-setup is two steps: a manager authenticates, then picks the branch.
enum SetupPhase { case credentials, pickBranch }

@MainActor
final class AppModel: ObservableObject {
    let core: SufrixCore
    private let vault = KeychainTokenStore()

    /// The active session, or `nil` when signed out. Drives the root route.
    @Published private(set) var session: SessionSnapshot?
    @Published private(set) var isBusy = false
    @Published var errorMessage: String?

    /// The device's configured branch (set once at provisioning). PIN login
    /// derives the org from it; post-D13 any active org teller may sign in here.
    @Published private(set) var branchId: String
    @Published private(set) var branchName: String

    /// Forces the device-setup (manager) view even on a configured device.
    @Published private(set) var reconfiguring = false
    @Published private(set) var setupPhase: SetupPhase = .credentials
    /// Branches fetched after the manager authenticates (the picker source).
    @Published private(set) var branches: [BranchView] = []
    /// Theme preference — defaults to light (the original navy palette).
    @Published var themeMode: ThemeMode {
        didSet { UserDefaults.standard.set(themeMode.rawValue, forKey: Self.themeKey) }
    }

    init() {
        Self.registerFonts()
        var cfg = defaultConfig()
        cfg.dbPath = Self.databasePath()
        cfg.locale = Locale.current.identifier
        // A failed store open is unrecoverable — fail loudly rather than limp on.
        core = try! SufrixCore(config: cfg)
        branchId = UserDefaults.standard.string(forKey: Self.branchKey) ?? ""
        branchName = UserDefaults.standard.string(forKey: Self.branchNameKey) ?? ""
        themeMode = ThemeMode(rawValue: UserDefaults.standard.string(forKey: Self.themeKey) ?? "") ?? .light

        core.setTokenStore(store: vault)
        if let blob = vault.loadBlob() {
            session = core.restoreSession(blob: blob)
        }
    }

    var isSignedIn: Bool { session != nil }
    /// Till bound to a branch → teller PIN login; until then, manager device-setup.
    var isBranchConfigured: Bool { !branchId.trimmingCharacters(in: .whitespaces).isEmpty }

    // ── teller ────────────────────────────────────────────────────────────────

    /// Teller sign-in (name + PIN). The core decides online vs offline.
    func signInTeller(name: String, pin: String) async {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            session = try await core.signIn(req: LoginRequest(
                mode: .pin, name: name, pin: pin, branchId: branchId,
                email: nil, password: nil, orgId: nil))
        } catch {
            errorMessage = humanMessage(error)
        }
    }

    // ── device setup (manager) ──────────────────────────────────────────────────

    /// Step 1: a manager authenticates (online), then we load the org's branches
    /// for the picker. The manager session is kept only to fetch branches; it's
    /// dropped when the branch is bound (the POS is teller-only).
    func authenticateManager(email: String, password: String) async {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            _ = try await core.login(req: LoginRequest(
                mode: .email, name: nil, pin: nil, branchId: nil,
                email: email, password: password, orgId: nil))
            branches = try await core.listBranches()
            setupPhase = .pickBranch
        } catch {
            errorMessage = humanMessage(error)
            try? core.logout(wipeOutbox: false)
            session = nil
        }
    }

    /// Step 2: bind the till to `branch`, drop the manager session, and leave the
    /// cached org bundle warm so tellers can unlock offline.
    func bindBranch(_ branch: BranchView) {
        branchId = branch.id
        branchName = branch.name
        UserDefaults.standard.set(branchId, forKey: Self.branchKey)
        UserDefaults.standard.set(branchName, forKey: Self.branchNameKey)
        try? core.logout(wipeOutbox: false)
        session = nil
        reconfiguring = false
        setupPhase = .credentials
        branches = []
        errorMessage = nil
    }

    func beginReconfigure() {
        reconfiguring = true; setupPhase = .credentials; branches = []; errorMessage = nil
    }
    func cancelReconfigure() {
        reconfiguring = false; setupPhase = .credentials; branches = []; errorMessage = nil
        try? core.logout(wipeOutbox: false)
        session = nil
    }

    func signOut() {
        try? core.logout(wipeOutbox: false)
        session = nil
        errorMessage = nil
    }

    // ── localization ────────────────────────────────────────────────────────────
    /// Localized UI string (from the core's shared i18n table).
    func t(_ key: String) -> String { core.tr(key: key) }
    /// Whether the active locale is right-to-left (host flips layout direction).
    var isRTL: Bool { core.isRtl() }

    // ── plumbing ────────────────────────────────────────────────────────────────

    /// Map the coarse `CoreError` to something a teller can read. Host-generated
    /// messages are localized; server-provided ones (auth/validation/server) pass
    /// through as the backend sent them.
    func humanMessage(_ error: Error) -> String {
        guard let e = error as? CoreError else { return error.localizedDescription }
        switch e {
        case .Offline: return t("err.offline_no_setup")
        case let .Unauthenticated(message): return message
        case let .Validation(_, message): return message
        case let .Server(_, _, message): return message
        case .Transient: return t("err.network")
        case .Forbidden: return t("err.not_allowed")
        case let .Internal(message): return message.isEmpty ? t("err.generic") : message
        }
    }

    private static let branchKey = "sufrix.branch_id"
    private static let branchNameKey = "sufrix.branch_name"
    private static let themeKey = "sufrix.theme"

    /// App-private SQLite path under Application Support.
    private static func databasePath() -> String {
        let fm = FileManager.default
        let dir = (try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask,
                               appropriateFor: nil, create: true))
            ?? fm.temporaryDirectory
        return dir.appendingPathComponent("sufrix.sqlite").path
    }

    /// Register the bundled Cairo faces so `Font.custom("Cairo-…")` resolves
    /// (the run-on-mac bundle ships them in Resources; the iOS app can also use
    /// Info.plist UIAppFonts). Best-effort — falls back to the system font.
    private static func registerFonts() {
        let faces = ["Cairo-Regular", "Cairo-Medium", "Cairo-SemiBold", "Cairo-Bold", "Cairo-ExtraBold"]
        for face in faces {
            if let url = Bundle.main.url(forResource: face, withExtension: "ttf") {
                CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
            }
        }
    }
}
