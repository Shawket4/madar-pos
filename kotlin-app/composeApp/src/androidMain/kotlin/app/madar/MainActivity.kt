package app.madar

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.madar.core.MadarCore
import app.madar.core.defaultConfig
import java.io.File
import java.util.Locale

// Android entry point. The per-ABI libmadar_core.so (from
// ../../rust-core/tool/build-android.sh) is packaged under src/androidMain/jniLibs
// and loaded automatically by the UniFFI binding via JNA.
class MainActivity : ComponentActivity() {
    /** Held for the app's lifetime so the LAN relay (Phase E) can RECEIVE multicast
     *  (mDNS) + broadcast (the UDP beacon); Android drops both without it. */
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Draw behind the (now transparent) system bars; App's root applies the
        // matching systemBars insets so content sits in the safe area.
        enableEdgeToEdge()
        acquireMulticastLock()
        requestNotificationPermission()
        val cfg = defaultConfig().copy(
            dbPath = File(filesDir, "madar.sqlite").absolutePath,
            locale = Locale.getDefault().toLanguageTag(),
        )
        val core = MadarCore(cfg)
        val vault = FileVault(filesDir)
        val player = AndroidRealtimePlayer(applicationContext)
        setContent { App(core, vault, player) }
    }

    // Android 13+ gates posting notifications on a runtime grant. Best-effort: a
    // denial just means no banners (the ping + in-app UI still fire).
    private fun requestNotificationPermission() {
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            runCatching {
                requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 0)
            }
        }
    }

    // Best-effort: a device with no Wi-Fi (or a denied service) still runs; the LAN
    // relay just falls back to unicast / the manual hub.
    private fun acquireMulticastLock() {
        runCatching {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            multicastLock = wifi.createMulticastLock("madar-lan").apply {
                setReferenceCounted(false)
                acquire()
            }
        }
    }

    override fun onDestroy() {
        runCatching { multicastLock?.takeIf { it.isHeld }?.release() }
        super.onDestroy()
    }
}

/**
 * File-backed host vault in app-private storage (sandboxed per app). Acceptable
 * interim. TODO: wrap with Android Keystore / EncryptedSharedPreferences for
 * at-rest encryption of the session blob.
 */
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
