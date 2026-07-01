package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.AmountField
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.Type
import app.madar.ui.StatusChip
import app.madar.ui.MadarButton
import app.madar.ui.MadarCard
import app.madar.ui.SectionHeader
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarMark
import app.madar.ui.MadarTextField
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.Responsive
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize

// Open-shift — the continuation of login: login confirms WHO you are, this
// confirms WHAT'S in the drawer. A name-first greeting, one isolated hero count
// field (auto-focused), one loud primary. Wide screens split into the same
// BrandPanel as login; phones show one calm centered column. Mirror of the
// SwiftUI OpenShiftView.
@Composable
fun OpenShiftScreen(model: AppModel) {
    val c = madarColors()
    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= Responsive.wide
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                BrandPanel(Modifier.weight(1f).fillMaxHeight())
                Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                    FormColumn(model, showLogo = false, modifier = Modifier.verticalScroll(rememberScrollState()))
                }
            }
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                FormColumn(model, showLogo = true, modifier = Modifier.verticalScroll(rememberScrollState()))
            }
        }

        // Top-pinned chrome so a teller WAITING on the open-shift screen still sees +
        // recovers connectivity / a genuine session expiry — not only on the order
        // screen. The auth-paused banner shows only when the cached JWT actually
        // expired (the core gates it now). Mirror of the SwiftUI OpenShiftView overlay.
        Column(
            Modifier.align(Alignment.TopCenter).fillMaxWidth()
                .padding(horizontal = Space.lg, vertical = Space.sm),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            if (!model.isOnline) {
                NoticeBanner(t("chrome.offline_banner"), ChipTone.WARNING, icon = "wifi.slash")
            }
            if (model.syncAuthPaused) {
                AuthPausedBanner { model.error = null; model.showReauth = true }
            }
        }
        // Self-gating re-auth sheet (renders only when model.showReauth). Order and
        // OpenShift are exclusive routes, so this never coexists with OrderScreen's.
        ReauthScreen(model)
    }
}

@Composable
private fun FormColumn(model: AppModel, showLogo: Boolean, modifier: Modifier = Modifier) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var openingMinor by remember { mutableStateOf(0L) }
    var reason by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    val suggested = model.suggestedOpeningCashMinor
    // The count deviates from the carried-over closing → a reason is required.
    val needsReason = suggested > 0L && openingMinor != suggested

    // Prime the prefill on entry; seed the count once while still untouched.
    // reconcileShift() FIRST — it adopts an already-open shift (opened earlier or
    // on another device) so a teller who lands here never opens a SECOND shift on
    // top of a live one. This matched the Swift OpenShiftView .task ordering but
    // was missing on Kotlin (the duplicate-shift bug). Both calls are suspend.
    LaunchedEffect(Unit) {
        model.clearError()
        model.reconcileShift()
        model.loadOpenShiftPrefill()
    }
    LaunchedEffect(suggested) {
        if (openingMinor == 0L && suggested > 0L) openingMinor = suggested
    }

    // Connectivity heartbeat: a teller who landed on open-shift while offline
    // re-adopts their active shift the moment the network returns (the
    // reconcile-on-reconnect lives inside refreshConnectivity). The loop is tied
    // to composition and cancels naturally when the screen leaves. Mirror of the
    // SwiftUI OpenShiftView's second .task.
    LaunchedEffect(Unit) {
        while (true) {
            model.refreshConnectivity()
            delay(15_000)
        }
    }

    fun submit() {
        if (needsReason && reason.isBlank()) {
            // Non-composable `model.t` (not the @Composable top-level `t`) — submit()
            // runs from an event handler, outside composition.
            model.flagError(model.t("shift.opening_reason_required"))
        } else {
            scope.launch { model.openShift(openingMinor, if (needsReason) reason else null) }
        }
    }

    Column(
        modifier.widthIn(max = 480.dp).fillMaxWidth()
            .padding(horizontal = Space.xxl, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (showLogo) MadarMark(size = 56.dp)

        // ── Greeting (the teller's name IS the hero) ──────────────────────────
        Column(
            Modifier.padding(top = if (showLogo) Space.xl else 0.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.xs),
        ) {
            Text(t("shift.welcome"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 15.sp)
            Text(
                model.session?.displayName ?: t("shift.open_title"),
                color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 28.sp, letterSpacing = (-0.5).sp, textAlign = TextAlign.Center,
            )
            if (model.branchName.isNotBlank()) {
                Box(Modifier.padding(top = Space.xs)) { StatusChip(model.branchName, ChipTone.INFO, icon = "building.2") }
            }
        }

        // ── Hero count field (the one thing the teller must do) ───────────────
        // Wrapped in the shared bordered surface card — matches the Order screen's
        // raised, hairline-bordered surfaces. Section-labelled, the hero figure sits
        // on its own elevated panel instead of floating on the page background.
        MadarCard(
            modifier = Modifier.padding(top = Space.xxl),
            spacing = Space.md,
        ) {
            SectionHeader(t("shift.opening_cash"), icon = "banknote")
            AmountField(
                amountMinor = openingMinor,
                onAmountMinor = { openingMinor = it },
                currencyCode = currency,
                autofocus = true,
            )

            // Carried-over suggestion (previous declared closing).
            if (suggested > 0L) CarryoverHint(suggested, currency)

            // Discrepancy reason — only when the count deviates from carryover.
            if (needsReason) {
                MadarTextField(reason, { reason = it }, t("shift.opening_reason_label"), icon = "exclamationmark.bubble")
            }

            Text(
                if (needsReason) t("shift.opening_reason_hint") else t("shift.opening_hint"),
                color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp, textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        // ── Error (next to the action that triggers it) ───────────────────────
        model.error?.let {
            Box(Modifier.fillMaxWidth().padding(top = Space.xl)) { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
        }

        // ── Primary action ────────────────────────────────────────────────────
        MadarButton(
            t("shift.open_button"),
            { submit() },
            modifier = Modifier.padding(top = if (model.error == null) Space.xl else Space.md),
            loading = model.isBusy,
            icon = "lock.open",
        )

        // ── Recessive exit ─────────────────────────────────────────────────────
        MadarButton(
            t("shift.switch_teller"),
            { model.signOut() },
            modifier = Modifier.padding(top = Space.sm),
            variant = BtnVariant.GHOST,
        )
    }
}

/** The carried-over opening-cash suggestion (previous declared closing) — a tinted
 *  teal block carrying the prior figure as bold teal money, the twin of
 *  CloseShift's ExpectedCashBlock (the figure this open count reconciles against).
 *  Mirror of the SwiftUI OpenShiftForm.CarryoverHint. */
@Composable
private fun CarryoverHint(suggestedMinor: Long, currency: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    Row(
        modifier.fillMaxWidth()
            .clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
            .padding(14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        MadarIcon("clock.arrow.circlepath", tint = c.accent, size = IconSize.sm)
        Text(t("shift.suggested_from_close"), color = c.accent, style = Type.label().copy(fontWeight = FontWeight.Bold))
        Box(Modifier.weight(1f))
        Text(Money.format(suggestedMinor, currency), color = c.accent, style = Type.money(20.sp, FontWeight.Black))
    }
}
