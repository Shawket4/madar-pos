package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import app.sufrix.core.AppRoute
import app.sufrix.ui.LocalLocalize
import app.sufrix.ui.SufrixTheme
import app.sufrix.ui.sufrixColors

// Shared Compose host (Android + desktop). Thin: the core decides the route
// (DeviceSetup → Login → OpenShift → Order); this only renders. Default theme is
// light; localization + RTL come from the core.
@Composable
fun App(model: AppModel) {
    SufrixTheme(mode = model.themeMode) {
        // Re-key on the active locale so a runtime language switch recomposes the
        // whole subtree — re-resolving every t() string and the RTL direction.
        key(model.locale) {
            CompositionLocalProvider(
                LocalLocalize provides { key -> model.t(key) },
                LocalLayoutDirection provides if (model.isRTL) LayoutDirection.Rtl else LayoutDirection.Ltr,
            ) {
                Box(Modifier.fillMaxSize().background(sufrixColors().bg)) {
                    when (model.route) {
                        AppRoute.DEVICE_SETUP, AppRoute.LOGIN -> LoginScreen(model)
                        AppRoute.OPEN_SHIFT -> OpenShiftScreen(model)
                        AppRoute.ORDER -> OrderScreen(model)
                    }
                }
            }
        }
    }
}

/** Convenience for entry points that hold the core + vault but not yet a model. */
@Composable
fun App(core: app.sufrix.core.SufrixCore, vault: HostVault) {
    val model = remember { AppModel(core, vault) }
    App(model)
}
