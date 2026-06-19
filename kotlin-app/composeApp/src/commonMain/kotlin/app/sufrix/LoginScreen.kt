package app.sufrix

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

// Login screen (PLAN §6). Two modes share one core entry point: tellers sign in
// with name + PIN (works online AND offline via the cached org bundle), managers
// with email + password (online only). The view never decides online vs offline
// — `core.signIn` does.
private enum class Mode(val label: String) { TELLER("Teller"), MANAGER("Manager") }

@Composable
fun LoginScreen(model: AppModel) {
    val scope = rememberCoroutineScope()
    var mode by remember { mutableStateOf(Mode.TELLER) }
    var name by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }

    val field = Modifier.fillMaxWidth().widthIn(max = 360.dp)

    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Sufrix POS", style = MaterialTheme.typography.headlineLarge)
        Spacer(Modifier.height(20.dp))

        TabRow(selectedTabIndex = mode.ordinal, modifier = Modifier.widthIn(max = 360.dp)) {
            Mode.entries.forEach { m ->
                Tab(selected = mode == m, onClick = { mode = m }, text = { Text(m.label) })
            }
        }
        Spacer(Modifier.height(16.dp))

        when (mode) {
            Mode.TELLER -> {
                OutlinedTextField(name, { name = it }, label = { Text("Name") },
                    singleLine = true, modifier = field)
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(pin, { pin = it }, label = { Text("PIN") }, singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                    modifier = field)
                Spacer(Modifier.height(8.dp))
                // Set once at device provisioning; persisted by the host vault.
                OutlinedTextField(model.branchId, { model.branchId = it },
                    label = { Text("Device branch ID") }, singleLine = true, modifier = field)
            }
            Mode.MANAGER -> {
                OutlinedTextField(email, { email = it }, label = { Text("Email") }, singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email), modifier = field)
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(password, { password = it }, label = { Text("Password") },
                    singleLine = true, visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = field)
            }
        }

        model.error?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }

        Spacer(Modifier.height(20.dp))
        val canSubmit = when (mode) {
            Mode.TELLER -> name.isNotBlank() && pin.isNotEmpty() && model.branchId.isNotBlank()
            Mode.MANAGER -> email.isNotBlank() && password.isNotEmpty()
        }
        Button(
            onClick = {
                scope.launch {
                    when (mode) {
                        Mode.TELLER -> model.signInTeller(name.trim(), pin)
                        Mode.MANAGER -> model.signInManager(email.trim(), password)
                    }
                }
            },
            enabled = !model.isBusy && canSubmit,
            modifier = field,
        ) {
            if (model.isBusy) CircularProgressIndicator(modifier = Modifier.height(20.dp))
            else Text("Sign in")
        }
    }
}
