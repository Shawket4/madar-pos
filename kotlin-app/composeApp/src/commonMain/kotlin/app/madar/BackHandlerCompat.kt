package app.madar

import androidx.compose.runtime.Composable

/**
 * Intercept the platform "back" gesture. On Android this is the system back button /
 * gesture (which otherwise pops the Activity = EXITS the app); on desktop there is no
 * system back, so it's a no-op. When [enabled] is false the event propagates (e.g. at
 * the true root, Android back exits as usual). Used to close the current overlay /
 * sub-screen instead of leaving the app.
 */
@Composable
expect fun BackHandlerCompat(enabled: Boolean, onBack: () -> Unit)
