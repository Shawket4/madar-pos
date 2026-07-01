package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.ChipTone
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarButton
import app.madar.ui.MadarIcon
import app.madar.ui.MadarSheet
import app.madar.ui.NoticeBanner
import app.madar.ui.PinPad
import app.madar.ui.Radii
import app.madar.ui.SheetSize
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.rememberHaptics
import app.madar.ui.t
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

    // The shared branded bottom sheet — slides in on a spring, dims the scrim,
    // drag/tap-scrim to dismiss; one sheet idiom across the app, matching Swift.
    MadarSheet(onDismiss = { app.showReauth = false }, size = SheetSize.HUG, maxWidth = 440.dp) { dismiss ->
        ReauthHeader(t("chrome.reauth_title"), t("chrome.reauth_body"), onClose = dismiss)
        // Deliberate rhythm mirrors the Login PIN pad (not a flat stack): the
        // identity pill sits up top, then `xxl` of air above the pad (and `xl`
        // below) so it reads as the hero, `sm` before the CTA, and a clear gap
        // down to the quiet escape hatch.
        Column(
            Modifier.fillMaxWidth().background(madarColors().surfaceAlt)
                .padding(horizontal = Space.lg, vertical = Space.xl),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // Locked to the current teller — no name field, just the shared
            // tinted-teal identity pill (same StatusChip the Login branch pill uses).
            StatusChip("${t("chrome.reauth_as")} $tellerName", ChipTone.ACCENT, icon = "person.crop.circle.badge.clock")

            Spacer(Modifier.height(Space.xxl))

            PinPad(pin, maxPin, onDigit = ::digit, onBackspace = { if (pin.isNotEmpty()) pin = pin.dropLast(1) })

            app.error?.let {
                Spacer(Modifier.height(Space.sm))
                NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle")
            }

            Spacer(Modifier.height(Space.xl))

            // The sign-in CTA carries the weight — bold teal fill, the brightest
            // thing on the sheet (mirrors the Login pad).
            MadarButton(t("login.sign_in"), { submit() }, loading = app.isBusy, height = 52.dp, icon = "arrow.right.circle")

            Spacer(Modifier.height(Space.sm))

            // Escape hatch — close the shift and route a different teller to login.
            SwitchTellerLink(onClick = { app.reauthSwitchTeller() })
        }
    }
}

/** The sheet header — a leading accent-tinted icon tile (the signature tone-tile
 *  pattern), the hero title + supporting body, and a trailing close affordance. */
@Composable
private fun ReauthHeader(title: String, body: String, onClose: () -> Unit, modifier: Modifier = Modifier) {
    val c = madarColors()
    Column(modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            Box(
                Modifier.size(44.dp).clip(RoundedCornerShape(Radii.md)).background(c.accentBg),
                contentAlignment = Alignment.Center,
            ) {
                MadarIcon("lock.circle", tint = c.accent, size = IconSize.lg)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs / 2)) {
                Text(title, style = Type.h2(), color = c.textPrimary, fontSize = 20.sp)
                Text(
                    body, style = Type.bodySm(), color = c.textSecondary, fontSize = 12.sp,
                    maxLines = 2, overflow = TextOverflow.Ellipsis,
                )
            }
            CloseButton(onClick = onClose)
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

/** The header's close glyph — a bordered surface-alt squircle (matches the order
 *  screen's bar-button idiom). */
@Composable
private fun CloseButton(onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Box(
        Modifier.size(32.dp).pressScale(interaction)
            .clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        MadarIcon("xmark", tint = c.textMuted, size = 14.dp)
    }
}

/** The quiet muted escape-hatch link beneath the CTA, with press feedback. */
@Composable
private fun SwitchTellerLink(onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Text(
        t("chrome.reauth_switch"), color = c.textMuted, fontFamily = LocalMadarFont.current,
        fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
        modifier = Modifier
            .pressScale(interaction)
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(vertical = Space.xs),
    )
}
