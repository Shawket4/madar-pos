package app.sufrix

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent

// Android entry point. The per-ABI libsufrix_core.so (from
// ../../rust-core/tool/build-android.sh) is packaged under src/androidMain/jniLibs
// and loaded automatically by the UniFFI binding via JNA.
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { App() }
    }
}
