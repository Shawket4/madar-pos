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
import app.sufrix.ui.ThemeMode

/**
 * Host secure-bytes vault — the core's [TokenStore] plus the host-only reads the
 * core doesn't push: the cold-start blob, the device's configured branch, and the
 * theme preference. Implemented per platform (Android filesDir / desktop home).
 */
interface HostVault : TokenStore {
    fun loadBlob(): ByteArray?
    var branchId: String
    var branchName: String
    var themeMode: String
}

/** Device-setup is two steps: a manager authenticates, then picks the branch. */
enum class SetupPhase { CREDENTIALS, PICK_BRANCH }

/**
 * The host's single source of UI state, shared by Android + desktop. Mirrors the
 * Swift `AppModel`. NO business logic — the online↔offline decision, token
 * custody, localization and validation all live in the core.
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
    /** Theme preference — defaults to LIGHT (the original navy palette). */
    var themeMode by mutableStateOf(
        runCatching { ThemeMode.valueOf(vault.themeMode) }.getOrDefault(ThemeMode.LIGHT)
    )
        private set

    init {
        core.setTokenStore(vault)
        vault.loadBlob()?.let { session = core.restoreSession(it) }
    }

    val isSignedIn: Boolean get() = session != null
    val isBranchConfigured: Boolean get() = branchId.isNotBlank()

    // ── localization ──────────────────────────────────────────────────────────
    fun t(key: String): String = core.tr(key)
    val isRTL: Boolean get() = core.isRtl()

    fun setThemeMode(mode: ThemeMode) {
        themeMode = mode
        vault.themeMode = mode.name
    }

    // ── teller ──────────────────────────────────────────────────────────────
    suspend fun signInTeller(name: String, pin: String) {
        isBusy = true; error = null
        try {
            session = core.signIn(LoginRequest(LoginMode.PIN, name, pin, branchId, null, null, null))
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isBusy = false
        }
    }

    // ── device setup (manager) ────────────────────────────────────────────────
    suspend fun authenticateManager(email: String, password: String) {
        isBusy = true; error = null
        try {
            core.login(LoginRequest(LoginMode.EMAIL, null, null, null, email, password, null))
            branches = core.listBranches()
            setupPhase = SetupPhase.PICK_BRANCH
        } catch (e: CoreException) {
            error = humanMessage(e); runCatching { core.logout(false) }; session = null
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic"); runCatching { core.logout(false) }; session = null
        } finally {
            isBusy = false
        }
    }

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

    /** Host-side errors localized; server messages pass through. */
    private fun humanMessage(e: CoreException): String = when (e) {
        is CoreException.Offline -> core.tr("err.offline_no_setup")
        is CoreException.Unauthenticated -> e.message ?: core.tr("err.generic")
        is CoreException.Validation -> e.message ?: core.tr("err.generic")
        is CoreException.Server -> e.message ?: core.tr("err.generic")
        is CoreException.Transient -> core.tr("err.network")
        is CoreException.Forbidden -> core.tr("err.not_allowed")
        is CoreException.Internal -> e.message?.takeIf { it.isNotBlank() } ?: core.tr("err.generic")
    }
}
