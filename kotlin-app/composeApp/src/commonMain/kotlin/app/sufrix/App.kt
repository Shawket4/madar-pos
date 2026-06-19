package app.sufrix

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Divider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import app.sufrix.core.SufrixCore
import app.sufrix.core.ffiSurfaceVersion
import app.sufrix.core.greet

// Phase-1 placeholder screen, shared by Android + desktop. Proves the core is
// reachable from Compose. Phase 6 replaces this with real layouts per form
// factor (PLAN.md §6). All logic stays in rust-core; this only renders.
@Composable
fun App() {
    // One core handle for the app lifetime. Phase 2 fills the db path + auth.
    val core = rememberCore()
    MaterialTheme {
        Surface(modifier = Modifier.fillMaxSize()) {
            Column(
                modifier = Modifier.fillMaxSize().padding(32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text("Sufrix POS", style = MaterialTheme.typography.headlineLarge)
                Text(greet(name = "Teller"), style = MaterialTheme.typography.bodyMedium)
                Divider(Modifier.padding(vertical = 12.dp))
                StatRow("core version", core.version())
                StatRow("ffi surface", ffiSurfaceVersion().toString())
                StatRow("environment", core.environment())
                StatRow("base URL", core.baseUrl())
            }
        }
    }
}

@Composable
private fun StatRow(label: String, value: String) {
    Row(Modifier.padding(vertical = 2.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, style = MaterialTheme.typography.labelMedium)
        Text(value, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.labelMedium)
    }
}

// `SufrixCore.fromEnv()` is the generated UniFFI constructor. Hold one handle
// for the whole composition.
@Composable
private fun rememberCore(): SufrixCore = remember { SufrixCore.fromEnv() }
