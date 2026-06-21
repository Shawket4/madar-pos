package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import app.sufrix.ui.AmountField
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.launch

// Open-shift — the continuation of login: login confirms WHO you are, this
// confirms WHAT'S in the drawer. A name-first greeting, one isolated hero count
// field (auto-focused), one loud primary. Wide screens split into the same
// BrandPanel as login; phones show one calm centered column. Mirror of the
// SwiftUI OpenShiftView.
@Composable
fun OpenShiftScreen(model: AppModel) {
    val c = sufrixColors()
    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= 760.dp
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                BrandPanel(Modifier.weight(1f).fillMaxHeight())
                Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                    FormColumn(model, showLogo = false)
                }
            }
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                FormColumn(model, showLogo = true)
            }
        }
    }
}

@Composable
private fun FormColumn(model: AppModel, showLogo: Boolean) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var openingMinor by remember { mutableStateOf(0L) }
    var reason by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    val suggested = model.suggestedOpeningCashMinor
    // The count deviates from the carried-over closing → a reason is required.
    val needsReason = suggested > 0L && openingMinor != suggested

    // Prime the prefill on entry; seed the count once while still untouched.
    LaunchedEffect(Unit) {
        model.clearError()
        model.loadOpenShiftPrefill()
    }
    LaunchedEffect(suggested) {
        if (openingMinor == 0L && suggested > 0L) openingMinor = suggested
    }

    fun submit() {
        if (needsReason && reason.isBlank()) {
            model.flagError(t("shift.opening_reason_required"))
        } else {
            scope.launch { model.openShift(openingMinor, if (needsReason) reason else null) }
        }
    }

    Column(
        Modifier.widthIn(max = 400.dp).fillMaxWidth().verticalScroll(rememberScrollState())
            .padding(horizontal = Space.xxl, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (showLogo) SufrixMark(size = 56.dp)

        // ── Greeting (the teller's name IS the hero) ──────────────────────────
        Column(
            Modifier.padding(top = if (showLogo) Space.xl else 0.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.xs),
        ) {
            Text(t("shift.welcome"), color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 15.sp)
            Text(
                model.session?.displayName ?: t("shift.open_title"),
                color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 28.sp, textAlign = TextAlign.Center,
            )
            if (model.branchName.isNotBlank()) {
                Box(Modifier.padding(top = Space.xs)) { StatusChip(model.branchName, ChipTone.INFO) }
            }
        }

        // ── Hero count field ─────────────────────────────────────────────────
        Column(
            Modifier.fillMaxWidth().padding(top = Space.xxl),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            Text(t("shift.opening_cash"), color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            AmountField(
                amountMinor = openingMinor,
                onAmountMinor = { openingMinor = it },
                currencyCode = currency,
                autofocus = true,
            )

            // Carried-over suggestion (previous declared closing).
            if (suggested > 0L) {
                Row(
                    Modifier.fillMaxWidth()
                        .clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
                        .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
                        .padding(horizontal = Space.md, vertical = Space.sm),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(t("shift.suggested_from_close"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
                    Box(Modifier.weight(1f))
                    Text(Money.format(suggested, currency), color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                }
            }

            // Discrepancy reason — only when the count deviates from carryover.
            if (needsReason) {
                SufrixTextField(reason, { reason = it }, t("shift.opening_reason_label"))
            }

            Text(
                if (needsReason) t("shift.opening_reason_hint") else t("shift.opening_hint"),
                color = c.textMuted, fontFamily = SufrixFont, fontSize = 12.sp, textAlign = TextAlign.Center,
            )
        }

        // ── Error (next to the action that triggers it) ───────────────────────
        model.error?.let {
            Box(Modifier.fillMaxWidth().padding(top = Space.xl)) { NoticeBanner(it, ChipTone.DANGER) }
        }

        // ── Primary action ────────────────────────────────────────────────────
        SufrixButton(
            t("shift.open_button"),
            { submit() },
            modifier = Modifier.padding(top = if (model.error == null) Space.xl else Space.md),
            loading = model.isBusy,
        )

        // ── Recessive exit ─────────────────────────────────────────────────────
        SufrixButton(
            t("shift.switch_teller"),
            { model.signOut() },
            modifier = Modifier.padding(top = Space.sm),
            variant = BtnVariant.GHOST,
        )
    }
}
