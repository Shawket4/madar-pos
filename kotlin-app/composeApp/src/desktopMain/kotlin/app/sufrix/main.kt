package app.sufrix

import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import app.sufrix.core.SufrixCore
import app.sufrix.core.defaultConfig
import java.io.File
import java.util.Locale

// Desktop (JVM) entry point. The UniFFI Kotlin binding loads libsufrix_core via
// JNA from java.library.path; build it with ../../rust-core (cdylib) and point
// -Djna.library.path at target/<profile>, or bundle it in resources.
fun main() = application {
    val dir = File(System.getProperty("user.home"), ".sufrix").apply { mkdirs() }
    val cfg = defaultConfig().copy(
        dbPath = File(dir, "sufrix.sqlite").absolutePath,
        locale = Locale.getDefault().toLanguageTag(),
    )
    val core = SufrixCore(cfg)
    val vault = FileVault(dir)
    Window(onCloseRequest = ::exitApplication, title = "Sufrix POS") {
        App(core, vault)
    }
}

/** File-backed host vault in the user's home dir (desktop has no Keychain). */
internal class FileVault(dir: File) : HostVault {
    private val blobFile = File(dir, "session.blob")
    private val branchFile = File(dir, "branch.txt")
    private val branchNameFile = File(dir, "branch_name.txt")
    private val themeFile = File(dir, "theme.txt")

    override fun saveBlob(blob: ByteArray) { blobFile.writeBytes(blob) }
    override fun clearBlob() { blobFile.delete() }
    override fun loadBlob(): ByteArray? = if (blobFile.exists()) blobFile.readBytes() else null

    override var branchId: String
        get() = if (branchFile.exists()) branchFile.readText() else ""
        set(value) { branchFile.writeText(value) }
    override var branchName: String
        get() = if (branchNameFile.exists()) branchNameFile.readText() else ""
        set(value) { branchNameFile.writeText(value) }
    override var themeMode: String
        get() = if (themeFile.exists()) themeFile.readText() else ""
        set(value) { themeFile.writeText(value) }
}
