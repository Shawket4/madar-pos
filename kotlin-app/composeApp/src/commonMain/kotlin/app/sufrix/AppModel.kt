package app.sufrix

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
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
}

/**
 * The host's single source of UI state, shared by Android + desktop. Owns the
 * one [SufrixCore] handle and the vault, mirrors the core's session into Compose
 * state, and forwards sign-in/out. NO business logic — the online↔offline
 * decision, token custody and validation all live in the core (`sign_in`).
 */
class AppModel(val core: SufrixCore, private val vault: HostVault) {
    var session by mutableStateOf<SessionSnapshot?>(null)
        private set
    var isBusy by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
    var branchId by mutableStateOf(vault.branchId)

    init {
        core.setTokenStore(vault)
        // Cold-start: re-hydrate the last session from the persisted blob.
        vault.loadBlob()?.let { session = core.restoreSession(it) }
    }

    val isSignedIn: Boolean get() = session != null

    suspend fun signInTeller(name: String, pin: String) = run {
        vault.branchId = branchId
        core.signIn(LoginRequest(LoginMode.PIN, name, pin, branchId, null, null, null))
    }

    suspend fun signInManager(email: String, password: String) = run {
        core.signIn(LoginRequest(LoginMode.EMAIL, null, null, null, email, password, null))
    }

    fun signOut() {
        runCatching { core.logout(false) }
        session = null
        error = null
    }

    private suspend fun run(op: suspend () -> SessionSnapshot) {
        isBusy = true
        error = null
        try {
            session = op()
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: "Unexpected error"
        } finally {
            isBusy = false
        }
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
