package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.ShiftReportView
import app.madar.core.ShiftView
import app.madar.ui.AmountField
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Opacity
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.MadarButton
import app.madar.ui.MadarTextField
import app.madar.ui.Type
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Elevation
import app.madar.ui.elevation
import kotlinx.coroutines.launch

// Close-shift — count the closing drawer and end the shift. Shown over the order
// screen; on a successful close the core marks the shift closed and the route
// flips back to open-shift. Card-based, mirror of the SwiftUI CloseShiftView.
@Composable
fun CloseShiftScreen(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var countedMinor by remember { mutableStateOf(0L) }
    var note by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    LaunchedEffect(Unit) { model.loadShiftReport() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        CloseShiftHeader(onBack = { model.error = null; model.showCloseShift = false })

        // ── Content ───────────────────────────────────────────────────────────
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.xl),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = 640.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(Space.lg),
            ) {
                model.shift?.let { SummaryCard(model, it, currency) }
                CashCard(countedMinor, { countedMinor = it }, note, { note = it }, currency, !model.isBusy, model.shiftReport)
                model.shiftReport?.let { ReportCard(it, currency) }
                if (model.shiftReport != null) {
                    // Preview the Z-report (paper layout) before printing — works with
                    // no printer, and the Print lives inside the preview.
                    MadarButton(
                        t("shift.print_report"),
                        { model.openShiftReportPreview() },
                        variant = BtnVariant.OUTLINE,
                        icon = "printer",
                    )
                }
                model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
                MadarButton(
                    t("order.close_shift"),
                    { scope.launch { model.closeShift(countedMinor, note) } },
                    variant = BtnVariant.DANGER,
                    loading = model.isBusy,
                    icon = "lock",
                )
            }
        }
    }
}

// ── Header ────────────────────────────────────────────────────────────────────
@Composable
private fun CloseShiftHeader(onBack: () -> Unit, modifier: Modifier = Modifier) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Column(modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            MadarIcon(
                "chevron.backward", tint = c.textPrimary, size = 17.dp,
                modifier = Modifier.pressScale(interaction)
                    .clickable(interactionSource = interaction, indication = null) { onBack() },
            )
            Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text(t("shift.close_title"), color = c.textPrimary, style = Type.h3().copy(fontWeight = FontWeight.Black, fontSize = 17.sp))
                Text(t("shift.closing_desc"), color = c.textSecondary, style = Type.bodySm().copy(fontSize = 12.sp))
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

@Composable
private fun SummaryCard(model: AppModel, shift: ShiftView, currency: String) {
    Card {
        CardHeader("doc.text", t("shift.summary"))
        InfoRow(t("shift.teller"), shift.tellerName)
        // Opening cash is money — give it the hero treatment (bold teal, tabular).
        InfoRow(t("shift.opening_cash"), Money.format(shift.openingCashMinor, currency), money = true)
        InfoRow(t("shift.opened_at"), model.fmtDateTime(shift.openedAt))
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
    Card {
        CardHeader("banknote", t("shift.counted_cash"))
        // System (expected) cash — the figure the count is measured against, so it
        // gets the hero money treatment in a tinted teal block (mirrors the order
        // screen's grand-total block).
        report?.let { ExpectedCashBlock(it.expectedCashMinor, currency) }
        AmountField(amountMinor = counted, onAmountMinor = onCounted, currencyCode = currency, autofocus = true)
        report?.let { DiscrepancyBanner(counted, it.expectedCashMinor, currency) }
        MadarTextField(note, onNote, t("shift.cash_note"), enabled = enabled, icon = "note.text")
    }
}

/** The system-expected cash — bold teal money in a tinted teal block, the figure
 *  the declared count is reconciled against. */
@Composable
private fun ExpectedCashBlock(expected: Long, currency: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    Row(
        modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
            .padding(horizontal = Space.lg, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(t("shift.system_cash"), color = c.accent, style = Type.label().copy(fontWeight = FontWeight.Bold))
            Text(t("shift.system_cash_explain"), color = c.textMuted, style = Type.labelSm().copy(fontWeight = FontWeight.Medium))
        }
        Text(Money.format(expected, currency), color = c.accent, style = Type.money(20.sp, FontWeight.Black))
    }
}

@Composable
private fun DiscrepancyBanner(declared: Long, expected: Long, currency: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    val diff = declared - expected
    val fg: Color = if (diff == 0L) c.success else if (diff > 0) c.warning else c.danger
    val bg: Color = if (diff == 0L) c.successBg else if (diff > 0) c.warningBg else c.dangerBg
    val glyph = if (diff == 0L) "checkmark.circle" else if (diff > 0) "arrow.up.circle" else "arrow.down.circle"
    val label = when {
        diff == 0L -> t("shift.drawer_matches")
        diff > 0 -> "${t("shift.drawer_over")} ${Money.format(diff, currency)}"
        else -> "${t("shift.drawer_short")} ${Money.format(-diff, currency)}"
    }
    Row(
        modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(bg)
            .border(1.dp, fg.copy(alpha = Opacity.border), RoundedCornerShape(Radii.sm))
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        MadarIcon(glyph, tint = fg, size = IconSize.md)
        Text(label, color = fg, style = Type.bodySm().copy(fontWeight = FontWeight.Medium))
    }
}

/** The Z-report breakdown: per-method sales (with proportional bars), drawer
 *  pay-in/out, voided total, the itemised cash movements, and totals. Mirrors
 *  the SwiftUI reportCard which embeds the shared ShiftReportBreakdown. */
@Composable
private fun ReportCard(r: ShiftReportView, currency: String) {
    Card {
        CardHeader("list.bullet.rectangle", t("shift.report_title"))
        ShiftReportBreakdown(r, currency)
    }
}

// ── Card primitives ──────────────────────────────────────────────────────────
@Composable
private fun Card(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val c = madarColors()
    Column(
        modifier.fillMaxWidth()
            .elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
            .clip(RoundedCornerShape(Radii.md))
            .background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md))
            .padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) { content() }
}

@Composable
private fun CardHeader(glyph: String, title: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    Row(
        modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        // Leading teal tone-tile behind the glyph — matches the confident
        // Kitchen/Order/Sync header (accentBg + accent icon, 34×34, Radii.sm).
        Box(
            Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon(glyph, tint = c.accent, size = IconSize.lg)
        }
        Text(title, color = c.textPrimary, style = Type.h3().copy(fontWeight = FontWeight.Bold))
    }
}

@Composable
private fun InfoRow(label: String, value: String, modifier: Modifier = Modifier, money: Boolean = false) {
    val c = madarColors()
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, style = Type.bodySm())
        Box(Modifier.weight(1f))
        // Money values are the hero — bold teal, tabular figures; everything else
        // stays a quiet semibold primary.
        if (money) {
            Text(value, color = c.accent, style = Type.money(14.sp, FontWeight.Bold))
        } else {
            Text(value, color = c.textPrimary, style = Type.bodySm().copy(fontWeight = FontWeight.SemiBold))
        }
    }
}
