package app.sufrix

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
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

// Shared Compose host (Android + desktop). Thin: it renders what the core hands
// it and routes Login ↔ signed-in home at deliberate boundaries only — never on
// connectivity (PLAN §R11). All logic stays in rust-core.
@Composable
fun App(model: AppModel) {
    MaterialTheme {
        Surface(modifier = Modifier.fillMaxSize()) {
            if (model.isSignedIn) HomeScreen(model) else LoginScreen(model)
        }
    }
}

/** Convenience for entry points that hold the core + vault but not yet a model. */
@Composable
fun App(core: app.sufrix.core.SufrixCore, vault: HostVault) {
    val model = remember { AppModel(core, vault) }
    App(model)
}

// Signed-in placeholder home. Proves the full auth round-trip from Compose:
// reads the cached session the core handed back and offers sign-out. Phase 6
// replaces this with the real Shift → Order → Cart → Payment screens (PLAN §6).
@Composable
private fun HomeScreen(model: AppModel) {
    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Signed in", style = MaterialTheme.typography.headlineLarge)
        Spacer(Modifier.height(12.dp))
        model.session?.let { s ->
            StatRow("teller", s.displayName)
            StatRow("role", s.role)
            StatRow("session", if (s.online) "online" else "offline")
            StatRow("currency", s.currencyCode)
        }
        Divider(Modifier.padding(vertical = 12.dp))
        StatRow("core version", model.core.version())
        StatRow("environment", model.core.environment())
        Spacer(Modifier.height(16.dp))
        Button(onClick = { model.signOut() }) { Text("Sign out") }
    }
}

@Composable
internal fun StatRow(label: String, value: String) {
    Row(Modifier.padding(vertical = 2.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, style = MaterialTheme.typography.labelMedium)
        Text(value, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.labelMedium)
    }
}
