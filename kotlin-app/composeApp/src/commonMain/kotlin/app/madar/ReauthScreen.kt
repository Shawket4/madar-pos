package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.ChipTone
import app.madar.ui.NoticeBanner
import app.madar.ui.PinPad
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.MadarButton
import app.madar.ui.MadarSheet
import app.madar.ui.SheetSize
import app.madar.ui.Type
import app.madar.ui.rememberHaptics
import app.madar.ui.LocalMadarFont
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import kotlinx.coroutines.launch

// Re-auth prompt shown when the bearer token expired mid-shift (`syncAuthPaused`).
// The teller who owns the OPEN shift re-enters their PIN to resume syncing — same
// teller, no handover (`reauth` un-parks the queue and drains the backlog). The
// escape hatch closes the shift and routes to the login screen for a new teller.
// Self-gating modal so OrderScreen can present it with a bare `ReauthScreen(model)`.
// Mirror of the SwiftUI ReauthView.
@Composable
fun ReauthScreen(app: AppModel) {
    if (!app.showReauth) return
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val haptics = rememberHaptics()
    var pin by remember { mutableStateOf("") }
    val maxPin = 6
    val tellerName = app.session?.displayName ?: ""

    fun submit() {
        if (pin.length < 4) { haptics.warning(); return }
        scope.launch {
            app.reauth(pin)
            if (app.error != null) { pin = ""; haptics.warning() }
        }
    }
    fun digit(d: String) {
        if (app.isBusy || pin.length >= maxPin) return
        app.error = null
        pin += d
        if (pin.length == maxPin) submit()
    }

    // The shared branded bottom sheet (was a hand-rolled centered card) — slides
    // in on a spring, dims the scrim, drag/tap-scrim to dismiss; one sheet idiom
    // across the app, matching Swift's madarSheet.
    MadarSheet(onDismiss = { app.showReauth = false }, size = SheetSize.HUG, maxWidth = 440.dp) { _ ->
        Column(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(t("chrome.reauth_title"), style = Type.h2(), color = c.textPrimary)
                Text(
                    t("chrome.reauth_body"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp,
                    maxLines = 2, overflow = TextOverflow.Ellipsis,
                )
            }
            // Locked to the current teller — no name field, just a "signed in as" chip.
            Row(
                Modifier.clip(CircleShape).background(c.accentBg).padding(horizontal = 12.dp, vertical = 7.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                MadarIcon("person.crop.circle.badge.clock", tint = c.accent, size = IconSize.xs)
                Text("${t("chrome.reauth_as")} $tellerName", color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            }
            PinPad(pin, maxPin, onDigit = ::digit, onBackspace = { if (pin.isNotEmpty()) pin = pin.dropLast(1) })
            app.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
            MadarButton(t("login.sign_in"), { submit() }, loading = app.isBusy)
            Text(
                t("chrome.reauth_switch"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                modifier = Modifier
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { app.reauthSwitchTeller() }
                    .padding(vertical = Space.xs),
            )
        }
    }
}
