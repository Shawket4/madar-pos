package app.madar

import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import app.madar.core.MadarCore
import app.madar.core.defaultConfig
import java.io.File
import java.util.Locale

// Desktop (JVM) entry point. The UniFFI Kotlin binding loads libmadar_core via
// JNA from java.library.path; build it with ../../rust-core (cdylib) and point
// -Djna.library.path at target/<profile>, or bundle it in resources.
fun main() = application {
    val dir = File(System.getProperty("user.home"), ".madar").apply { mkdirs() }
    val cfg = defaultConfig().copy(
        dbPath = File(dir, "madar.sqlite").absolutePath,
        locale = Locale.getDefault().toLanguageTag(),
    )
    val core = MadarCore(cfg)
    val vault = FileVault(dir)
    val player = DesktopRealtimePlayer()
    Window(onCloseRequest = ::exitApplication, title = "Madar POS") {
        App(core, vault, player)
    }
}

/** File-backed host vault in the user's home dir (desktop has no Keychain). */
internal class FileVault(dir: File) : HostVault {
    private val blobFile = File(dir, "session.blob")
    private val branchFile = File(dir, "branch.txt")
    private val branchNameFile = File(dir, "branch_name.txt")
    private val orgLogoFile = File(dir, "org_logo_url.txt")
    private val themeFile = File(dir, "theme.txt")
    private val localeFile = File(dir, "locale.txt")
    private val printerFile = File(dir, "printer.txt")
    private val printerBrandFile = File(dir, "printer_brand.txt")

    override fun saveBlob(blob: ByteArray) { blobFile.writeBytes(blob) }
    override fun clearBlob() { blobFile.delete() }
    override fun loadBlob(): ByteArray? = if (blobFile.exists()) blobFile.readBytes() else null

    override var branchId: String
        get() = if (branchFile.exists()) branchFile.readText() else ""
        set(value) { branchFile.writeText(value) }
    override var branchName: String
        get() = if (branchNameFile.exists()) branchNameFile.readText() else ""
        set(value) { branchNameFile.writeText(value) }
    override var orgLogoUrl: String?
        get() = if (orgLogoFile.exists()) orgLogoFile.readText().ifBlank { null } else null
        set(value) { if (value.isNullOrBlank()) orgLogoFile.delete() else orgLogoFile.writeText(value) }
    override var themeMode: String
        get() = if (themeFile.exists()) themeFile.readText() else ""
        set(value) { themeFile.writeText(value) }
    override var locale: String
        get() = if (localeFile.exists()) localeFile.readText() else ""
        set(value) { localeFile.writeText(value) }
    override var printerHost: String
        get() = if (printerFile.exists()) printerFile.readText() else ""
        set(value) { printerFile.writeText(value) }
    override var printerBrand: String
        get() = if (printerBrandFile.exists()) printerBrandFile.readText() else ""
        set(value) { printerBrandFile.writeText(value) }
}
