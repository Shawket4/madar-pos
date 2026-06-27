package app.madar

import androidx.compose.runtime.Composable

@Composable
actual fun BackHandlerCompat(enabled: Boolean, onBack: () -> Unit) {
    // The Android system back / predictive-back gesture. While enabled it routes here
    // (closing the current overlay) instead of popping the Activity out of the app.
    androidx.activity.compose.BackHandler(enabled = enabled, onBack = onBack)
}
