package app.sufrix

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.sufrix.core.BranchView
import app.sufrix.core.CoreException
import app.sufrix.core.LoginMode
import app.sufrix.core.LoginRequest
import app.sufrix.core.SessionSnapshot
import app.sufrix.core.SufrixCore
import app.sufrix.core.TokenStore

/**
 * Host secure-bytes vault — the core's [TokenStore] plus the host-only reads the
 * core doesn't push: the cold-start blob and the device's configured branch.
 * Implemented per platform (Android app-private storage / desktop home dir).
 */
interface HostVault : TokenStore {
    /** Read the persisted session blob once at launch to re-hydrate. */
    fun loadBlob(): ByteArray?
    /** The device's configured branch (set once at provisioning), persisted. */
    var branchId: String
    var branchName: String
}

/** Device-setup is two steps: a manager authenticates, then picks the branch. */
enum class SetupPhase { CREDENTIALS, PICK_BRANCH }

/**
 * The host's single source of UI state, shared by Android + desktop. Owns the
 * one [SufrixCore] handle and the vault, mirrors the core's session into Compose
 * state, and forwards sign-in/out. NO business logic — the online↔offline
 * decision, token custody and validation all live in the core.
 */
class AppModel(val core: SufrixCore, private val vault: HostVault) {
    var session by mutableStateOf<SessionSnapshot?>(null)
        private set
    var isBusy by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
    var branchId by mutableStateOf(vault.branchId)
        private set
    var branchName by mutableStateOf(vault.branchName)
        private set
    var reconfiguring by mutableStateOf(false)
        private set
    var setupPhase by mutableStateOf(SetupPhase.CREDENTIALS)
        private set
    var branches by mutableStateOf<List<BranchView>>(emptyList())
        private set

    init {
        core.setTokenStore(vault)
        vault.loadBlob()?.let { session = core.restoreSession(it) }
    }

    val isSignedIn: Boolean get() = session != null
    /** Till bound to a branch → teller PIN login; until then, manager device-setup. */
    val isBranchConfigured: Boolean get() = branchId.isNotBlank()

    // ── teller ──────────────────────────────────────────────────────────────
    /** Teller sign-in (name + PIN). The core decides online vs offline. */
    suspend fun signInTeller(name: String, pin: String) {
        isBusy = true; error = null
        try {
            session = core.signIn(LoginRequest(LoginMode.PIN, name, pin, branchId, null, null, null))
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: "Unexpected error"
        } finally {
            isBusy = false
        }
    }

    // ── device setup (manager) ────────────────────────────────────────────────
    /** Step 1: manager authenticates online, then we load branches for the picker. */
    suspend fun authenticateManager(email: String, password: String) {
        isBusy = true; error = null
        try {
            core.login(LoginRequest(LoginMode.EMAIL, null, null, null, email, password, null))
            branches = core.listBranches()
            setupPhase = SetupPhase.PICK_BRANCH
        } catch (e: CoreException) {
            error = humanMessage(e)
            runCatching { core.logout(false) }; session = null
        } catch (e: Exception) {
            error = e.message ?: "Unexpected error"
            runCatching { core.logout(false) }; session = null
        } finally {
            isBusy = false
        }
    }

    /** Step 2: bind the till to [branch], drop the manager session. */
    fun bindBranch(branch: BranchView) {
        branchId = branch.id
        branchName = branch.name
        vault.branchId = branchId
        vault.branchName = branchName
        runCatching { core.logout(false) }
        session = null
        reconfiguring = false
        setupPhase = SetupPhase.CREDENTIALS
        branches = emptyList()
        error = null
    }

    fun beginReconfigure() {
        reconfiguring = true; setupPhase = SetupPhase.CREDENTIALS; branches = emptyList(); error = null
    }
    fun cancelReconfigure() {
        reconfiguring = false; setupPhase = SetupPhase.CREDENTIALS; branches = emptyList(); error = null
        runCatching { core.logout(false) }; session = null
    }

    fun signOut() {
        runCatching { core.logout(false) }
        session = null
        error = null
    }
}

/** Map the coarse [CoreException] to something a teller can read. */
fun humanMessage(e: CoreException): String = when (e) {
    is CoreException.Offline ->
        "You're offline and this teller hasn't been set up for offline sign-in yet."
    is CoreException.Unauthenticated -> e.message ?: "Sign-in failed"
    is CoreException.Validation -> e.message ?: "Invalid input"
    is CoreException.Server -> e.message ?: "Server error"
    is CoreException.Transient -> "Network problem: ${e.message}"
    is CoreException.Forbidden -> "Not allowed"
    is CoreException.Internal -> "Something went wrong: ${e.message}"
}
