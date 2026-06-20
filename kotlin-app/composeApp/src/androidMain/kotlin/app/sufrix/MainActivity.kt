package app.sufrix

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import app.sufrix.core.SufrixCore
import app.sufrix.core.defaultConfig
import java.io.File
import java.util.Locale

// Android entry point. The per-ABI libsufrix_core.so (from
// ../../rust-core/tool/build-android.sh) is packaged under src/androidMain/jniLibs
// and loaded automatically by the UniFFI binding via JNA.
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val cfg = defaultConfig().copy(
            dbPath = File(filesDir, "sufrix.sqlite").absolutePath,
            locale = Locale.getDefault().toLanguageTag(),
        )
        val core = SufrixCore(cfg)
        val vault = FileVault(filesDir)
        setContent { App(core, vault) }
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
