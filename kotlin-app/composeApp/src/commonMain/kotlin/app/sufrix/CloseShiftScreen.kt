package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.ShiftReportView
import app.sufrix.core.ShiftView
import app.sufrix.ui.AmountField
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.backGlyph
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.launch

// Close-shift — count the closing drawer and end the shift. Shown over the order
// screen; on a successful close the core marks the shift closed and the route
// flips back to open-shift. Card-based, mirror of the SwiftUI CloseShiftView.
@Composable
fun CloseShiftScreen(model: AppModel) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var countedMinor by remember { mutableStateOf(0L) }
    var note by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    LaunchedEffect(Unit) { model.loadShiftReport() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // ── Header ────────────────────────────────────────────────────────────
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Text(
                    backGlyph(), color = c.textPrimary, fontSize = 26.sp,
                    modifier = Modifier.clickable { model.error = null; model.showCloseShift = false },
                )
                Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    Text(t("shift.close_title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
                    Text(t("shift.closing_desc"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        // ── Content ───────────────────────────────────────────────────────────
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.xl),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = 480.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(Space.lg),
            ) {
                model.shift?.let { SummaryCard(it, currency) }
                CashCard(countedMinor, { countedMinor = it }, note, { note = it }, currency, !model.isBusy, model.shiftReport)
                if (model.shiftReport != null) {
                    SufrixButton(
                        t("shift.print_report"),
                        { scope.launch { model.printShiftReport() } },
                        variant = BtnVariant.OUTLINE,
                        loading = model.printState == PrintState.PRINTING,
                    )
                }
                model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
                SufrixButton(
                    t("order.close_shift"),
                    { scope.launch { model.closeShift(countedMinor, note) } },
                    variant = BtnVariant.DANGER,
                    loading = model.isBusy,
                )
            }
        }
    }
}

@Composable
private fun SummaryCard(shift: ShiftView, currency: String) {
    Card {
        CardHeader("▤", t("shift.summary"))
        InfoRow(t("shift.teller"), shift.tellerName)
        InfoRow(t("shift.opening_cash"), Money.format(shift.openingCashMinor, currency))
        InfoRow(t("shift.opened_at"), formatWhen(shift.openedAt))
    }
}

@Composable
private fun CashCard(
    counted: Long,
    onCounted: (Long) -> Unit,
    note: String,
    onNote: (String) -> Unit,
    currency: String,
    enabled: Boolean,
    report: ShiftReportView?,
) {
    val c = sufrixColors()
    Card {
        CardHeader("¤", t("shift.counted_cash"))
        report?.let { r ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt).padding(Space.md),
                verticalAlignment = Alignment.Top,
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(t("shift.system_cash"), color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                    Text(t("shift.system_cash_explain"), color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)
                }
                Text(Money.format(r.expectedCashMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 18.sp)
            }
        }
        AmountField(amountMinor = counted, onAmountMinor = onCounted, currencyCode = currency, autofocus = true)
        report?.let { DiscrepancyBanner(counted, it.expectedCashMinor, currency) }
        SufrixTextField(note, onNote, t("shift.cash_note"), enabled = enabled)
    }
}

@Composable
private fun DiscrepancyBanner(declared: Long, expected: Long, currency: String) {
    val c = sufrixColors()
    val diff = declared - expected
    val color = if (diff == 0L) c.success else if (diff > 0) c.warning else c.danger
    val glyph = if (diff == 0L) "✓" else if (diff > 0) "▲" else "▼"
    val label = when {
        diff == 0L -> t("shift.drawer_matches")
        diff > 0 -> "${t("shift.drawer_over")} ${Money.format(diff, currency)}"
        else -> "${t("shift.drawer_short")} ${Money.format(-diff, currency)}"
    }
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(color.copy(alpha = 0.12f))
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text(glyph, color = color, fontSize = 14.sp)
        Text(label, color = color, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

// ── Card primitives ──────────────────────────────────────────────────────────
@Composable
private fun Card(content: @Composable () -> Unit) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) { content() }
}

@Composable
private fun CardHeader(glyph: String, title: String) {
    val c = sufrixColors()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
        Box(
            Modifier.size(36.dp).clip(RoundedCornerShape(Radii.xs)).background(c.navyBg),
            contentAlignment = Alignment.Center,
        ) {
            Text(glyph, color = c.navy, fontSize = 16.sp)
        }
        Text(title, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Text(value, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

/** "2026-06-20T12:00:00+00:00" → "2026-06-20 12:00". */
private fun formatWhen(rfc3339: String): String = rfc3339.replace('T', ' ').take(16)
