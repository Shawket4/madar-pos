package app.sufrix

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
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import app.sufrix.core.CashMovementView
import app.sufrix.core.ShiftSummaryView
import app.sufrix.ui.AmountField
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.backGlyph
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlin.math.abs
import kotlinx.coroutines.launch

// Cash In/Out + Past Shifts — two online-only manager screens reached from the
// "More" drawer. Cash movements record a signed pay-in / pay-out against the open
// shift (never queued); Past Shifts lists the branch's shift history. All data +
// rules live in the core; these screens collect input and render. Full-screen over
// the order screen. Mirror of the SwiftUI CashMovementsView + ShiftHistoryView.

// ── Cash In/Out ───────────────────────────────────────────────────────────────────
@Composable
fun CashMovementsScreen(model: AppModel) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var isIn by remember { mutableStateOf(true) }
    var amountMinor by remember { mutableStateOf(0L) }
    var note by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    val canRecord = amountMinor > 0L && !model.isBusy
    LaunchedEffect(Unit) { model.loadCashMovements() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        ScreenHeader(t("cash.title")) { model.showCashMovements = false }

        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = 520.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(Space.lg),
            ) {
                model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
                RecordCard(
                    isIn, { isIn = it }, amountMinor, { amountMinor = it }, note, { note = it },
                    currency, model.isBusy, canRecord,
                ) {
                    scope.launch {
                        val signed = if (isIn) amountMinor else -amountMinor
                        if (model.recordCashMovement(signed, note)) {
                            amountMinor = 0L; note = ""
                        }
                    }
                }
                MovementsList(model.cashMovements, currency)
            }
        }
    }
}

@Composable
private fun RecordCard(
    isIn: Boolean,
    onDirection: (Boolean) -> Unit,
    amountMinor: Long,
    onAmount: (Long) -> Unit,
    note: String,
    onNote: (String) -> Unit,
    currency: String,
    busy: Boolean,
    canRecord: Boolean,
    onRecord: () -> Unit,
) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            DirectionChip(t("cash.in"), isIn, c.success, Modifier.weight(1f)) { onDirection(true) }
            DirectionChip(t("cash.out"), !isIn, c.danger, Modifier.weight(1f)) { onDirection(false) }
        }
        AmountField(amountMinor = amountMinor, onAmountMinor = onAmount, currencyCode = currency)
        SufrixTextField(note, onNote, t("cash.note"))
        SufrixButton(t("cash.record"), onRecord, loading = busy, enabled = canRecord)
    }
}

@Composable
private fun DirectionChip(label: String, active: Boolean, tone: Color, modifier: Modifier, onClick: () -> Unit) {
    val c = sufrixColors()
    Box(
        modifier.clip(RoundedCornerShape(Radii.sm))
            .background(if (active) tone else c.surfaceAlt)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onClick() }
            .padding(vertical = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label, color = if (active) c.textOnAccent else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = FontWeight.Bold, fontSize = 13.sp,
        )
    }
}

@Composable
private fun MovementsList(movements: List<CashMovementView>, currency: String) {
    val c = sufrixColors()
    SectionTitle(t("cash.history"))
    if (movements.isEmpty()) {
        Box(Modifier.fillMaxWidth().padding(vertical = Space.lg), contentAlignment = Alignment.Center) {
            Text(t("cash.empty"), color = c.textMuted, fontFamily = SufrixFont, fontSize = 13.sp)
        }
    } else {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            movements.forEach { m -> MovementRow(m, currency) }
        }
    }
}

@Composable
private fun MovementRow(m: CashMovementView, currency: String) {
    val c = sufrixColors()
    val positive = m.amountMinor >= 0L
    val tone = if (positive) c.success else c.danger
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Text(if (positive) "▼" else "▲", color = tone, fontSize = 18.sp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                m.note.ifEmpty { m.movedByName }, color = c.textPrimary, fontFamily = SufrixFont,
                fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1,
            )
            Text(m.movedByName, color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)
        }
        Text(
            "${if (positive) "+" else "−"}${Money.format(abs(m.amountMinor), currency)}",
            color = tone, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp,
        )
    }
}

@Composable
private fun SectionTitle(label: String) {
    val c = sufrixColors()
    Text(
        label.uppercase(), color = c.textMuted, fontFamily = SufrixFont,
        fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
    )
}

// ── Past shifts ────────────────────────────────────────────────────────────────────
@Composable
fun ShiftHistoryScreen(model: AppModel) {
    val c = sufrixColors()
    val currency = model.session?.currencyCode ?: ""
    LaunchedEffect(Unit) { model.loadShiftHistory() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        ScreenHeader(t("shifts.title")) { model.showShiftHistory = false }

        if (model.shiftHistory.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    Text("↺", color = c.textMuted, fontSize = 36.sp)
                    Text(t("shifts.empty"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
                }
            }
        } else {
            LazyColumn(
                Modifier.fillMaxSize(),
                contentPadding = PaddingValues(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                items(model.shiftHistory, key = { it.id }) { s ->
                    Box(Modifier.widthIn(max = 560.dp).fillMaxWidth()) { ShiftRow(s, currency) }
                }
            }
        }
    }
}

@Composable
private fun ShiftRow(s: ShiftSummaryView, currency: String) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(shortDate(s.openedAt), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
            Box(Modifier.weight(1f))
            StatusChip(
                if (s.isOpen) t("shifts.open_now") else t("shifts.closed"),
                if (s.isOpen) ChipTone.SUCCESS else ChipTone.NEUTRAL,
            )
        }
        Metric(t("shifts.opening"), Money.format(s.openingCashMinor, currency))
        s.closingDeclaredMinor?.let { Metric(t("shifts.declared"), Money.format(it, currency)) }
        s.discrepancyMinor?.takeIf { it != 0L }?.let { disc ->
            Metric(
                t("shifts.discrepancy"),
                "${if (disc > 0L) "+" else "−"}${Money.format(abs(disc), currency)}",
                valueColor = c.danger,
            )
        }
    }
}

@Composable
private fun Metric(label: String, value: String, valueColor: Color? = null) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 12.sp)
        Box(Modifier.weight(1f))
        Text(value, color = valueColor ?: c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

// ── Shared back-chevron header ──────────────────────────────────────────────────────
@Composable
private fun ScreenHeader(title: String, onClose: () -> Unit) {
    val c = sufrixColors()
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            Text(backGlyph(), color = c.textPrimary, fontSize = 26.sp, modifier = Modifier.clickable { onClose() })
            Text(title, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

/** Trim an RFC3339 timestamp to "YYYY-MM-DD HH:MM" for the row title. */
private fun shortDate(rfc3339: String): String = rfc3339.replace("T", " ").take(16)
