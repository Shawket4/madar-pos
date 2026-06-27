package app.madar

import androidx.compose.runtime.Composable

@Composable
actual fun BackHandlerCompat(enabled: Boolean, onBack: () -> Unit) {
    // Desktop has no system back gesture (the window has its own close button), so
    // there's nothing to intercept — overlays are dismissed via their on-screen back
    // controls. No-op.
}
