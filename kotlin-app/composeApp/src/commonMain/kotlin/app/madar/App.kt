package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import app.madar.core.AppRoute
import app.madar.ui.LocalLocalize
import app.madar.ui.MadarTheme
import app.madar.ui.RealtimeAlertStack
import app.madar.ui.ToastHost
import app.madar.ui.madarColors

// Shared Compose host (Android + desktop). Thin: the core decides the route
// (DeviceSetup → Login → OpenShift → Order); this only renders. Default theme is
// light; localization + RTL come from the core.
@Composable
fun App(model: AppModel) {
    // Warm Coil's disk cache with the org logo while online, so the receipt can
    // render it even after a long offline stretch (mirrors the Swift prefetch).
    val coilCtx = coil3.compose.LocalPlatformContext.current
    androidx.compose.runtime.LaunchedEffect(model.orgLogoUrl, model.isOnline) {
        val url = model.orgLogoUrl
        if (model.isOnline && !url.isNullOrBlank()) {
            coil3.SingletonImageLoader.get(coilCtx)
                .enqueue(coil3.request.ImageRequest.Builder(coilCtx).data(url).build())
        }
    }
    // App-level connectivity heartbeat — runs on EVERY route (not just Order /
    // OpenShift), so a KDS / waiter / settings device still drains its outbox on a
    // timer, and a cold start flushes a restored backlog on the first tick. Keyed
    // on the signed-in user so it starts/stops with the session; the core's
    // single-flight drain makes overlap with any screen-level refresh harmless.
    LaunchedEffect(model.session?.userId) {
        if (model.session == null) return@LaunchedEffect
        while (true) {
            model.refreshConnectivity()
            kotlinx.coroutines.delay(15_000)
        }
    }
    // Drain the outbox when the realtime stream connects / reconnects — a regained
    // stream is strong proof connectivity is back. Covers every screen/role (incl.
    // KDS/waiter, which lack a per-screen heartbeat) and closes the prior on-reconnect
    // parity gap with the Swift app. Single-flight in the core de-dups any overlap.
    LaunchedEffect(model.realtimeConnected) {
        if (model.realtimeConnected && model.session != null) model.refreshConnectivity()
    }
    MadarTheme(mode = model.themeMode) {
        // Re-key on the active locale so a runtime language switch recomposes the
        // whole subtree — re-resolving every t() string and the RTL direction.
        key(model.locale) {
            CompositionLocalProvider(
                LocalLocalize provides { key -> model.t(key) },
                LocalLayoutDirection provides if (model.isRTL) LayoutDirection.Rtl else LayoutDirection.Ltr,
            ) {
                // Background is full-bleed (paints behind the transparent system
                // bars); content is inset to the safe area so nothing sits under
                // the status/navigation bars. systemBars insets are zero on
                // desktop, so this is a no-op there. (IME stays per-screen.)
                // System back closes the topmost overlay/sub-screen instead of exiting
                // the app. Disabled at the true root (no overlay) → back exits as usual.
                BackHandlerCompat(enabled = model.hasOverlay) { model.goBack() }
                Box(Modifier.fillMaxSize().background(madarColors().bg)) {
                    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars)) {
                        when (val r = model.route) {
                            // A kitchen-role device bound to a branch but with no
                            // station yet lands on DeviceSetup → show the station
                            // picker (not the manager binding, which is for an
                            // unconfigured device / a fresh teller login).
                            is AppRoute.DeviceSetup ->
                                if (model.session != null && model.isKitchenDevice) StationPickerScreen(model)
                                else LoginScreen(model)
                            is AppRoute.Login -> LoginScreen(model)
                            is AppRoute.OpenShift -> OpenShiftScreen(model)
                            // The waiter uses the SAME order component as the teller —
                            // full menu/cart + app chrome — in "fire" mode. Its open-
                            // tickets list is a sub-screen reached from the top bar.
                            is AppRoute.Order, is AppRoute.WaiterTickets -> OrderScreen(model)
                            is AppRoute.KitchenDisplay -> KitchenDisplayScreen(model, r.stationId)
                        }
                        // Toast layer — last child so it draws over the route + any sheets.
                        ToastHost(model.toast, onAction = { model.runToastAction() }, onDismiss = { model.dismissToast(it) })
                        // In-app realtime alert stack — top-anchored iOS-style deck,
                        // the visual companion to the OS notification (chime + haptic).
                        RealtimeAlertStack(
                            model.realtimeAlerts,
                            onDismiss = { model.dismissRealtimeAlert(it) },
                            onOpen = { model.openOrdersFromAlert(it) },
                            modifier = Modifier.align(Alignment.TopCenter),
                        )
                    }
                }
            }
        }
    }
}

/** Convenience for entry points that hold the core + vault but not yet a model. The
 *  platform supplies the [player] (Android: notifications/sound/vibrate; desktop: beep). */
@Composable
fun App(core: app.madar.core.MadarCore, vault: HostVault, player: app.madar.core.RealtimePlayer) {
    val model = remember { AppModel(core, vault, player) }
    // ONE session-level realtime subscription: starts on login AND on a cold boot
    // that restored a session (keyed on the signed-in user). The core auto-reconnects
    // thereafter; signOut tears it down. Replaces the old per-screen subscriptions.
    LaunchedEffect(model.session?.userId) {
        if (model.session != null) { model.startRealtime(); model.startLanRelay() }
    }
    App(model)
}
