package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
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
import app.madar.core.CashMovementView
import app.madar.core.OrderSummaryView
import app.madar.core.ShiftSummaryView
import app.madar.core.ShiftView
import app.madar.ui.AmountField
import app.madar.ui.ChipTone
import app.madar.ui.EmptyState
import app.madar.ui.MadarButton
import app.madar.ui.MadarCard
import app.madar.ui.MadarIcon
import app.madar.ui.MadarTextField
import app.madar.ui.MetricRow
import app.madar.ui.Money
import app.madar.ui.Motion
import app.madar.ui.NoticeBanner
import app.madar.ui.IconSize
import app.madar.ui.Radii
import app.madar.ui.Responsive
import app.madar.ui.ScreenHeader
import app.madar.ui.SectionHeader
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.screenHeaderBar
import app.madar.ui.t
import kotlin.math.abs
import kotlinx.coroutines.launch
import androidx.compose.material3.CircularProgressIndicator

// Cash In/Out + Past Shifts — two manager screens reached from the
// "More" drawer. Cash movements record a signed pay-in / pay-out against the open
// shift — OFFLINE-FIRST (queued through the durable outbox, idempotent on a
// client_ref); Past Shifts lists the branch's shift history. All data +
// rules live in the core; these screens collect input and render. Full-screen over
// the order screen. Mirror of the SwiftUI CashMovementsView + ShiftHistoryView.

// ── Cash In/Out ───────────────────────────────────────────────────────────────────
@Composable
fun CashMovementsScreen(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var isIn by remember { mutableStateOf(true) }
    var amountMinor by remember { mutableStateOf(0L) }
    var note by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    val canRecord = amountMinor > 0L && !model.isBusy
    LaunchedEffect(Unit) { model.loadCashMovements() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        Box(Modifier.screenHeaderBar()) {
            ScreenHeader(t("cash.title"), onBack = { model.showCashMovements = false })
        }

        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = 560.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(Space.lg),
            ) {
                model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
                if (model.cashMovements.isNotEmpty()) {
                    val totalIn = model.cashMovements.filter { it.amountMinor > 0 }.sumOf { it.amountMinor }
                    val totalOut = model.cashMovements.filter { it.amountMinor < 0 }.sumOf { -it.amountMinor }
                    CashSummaryStrip(totalIn, totalOut, totalIn - totalOut, currency)
                }
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
    val c = madarColors()
    MadarCard {
        Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            DirectionChip(t("cash.in"), isIn, c.success, Modifier.weight(1f)) { onDirection(true) }
            DirectionChip(t("cash.out"), !isIn, c.danger, Modifier.weight(1f)) { onDirection(false) }
        }
        AmountField(amountMinor = amountMinor, onAmountMinor = onAmount, currencyCode = currency)
        MadarTextField(note, onNote, t("cash.note"), icon = "text.bubble")
        MadarButton(t("cash.record"), onRecord, loading = busy, enabled = canRecord, icon = "plus.forwardslash.minus")
    }
}

@Composable
private fun DirectionChip(label: String, active: Boolean, tone: Color, modifier: Modifier, onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Box(
        modifier.pressScale(interaction, 0.97f).clip(RoundedCornerShape(Radii.sm))
            .background(if (active) tone else c.surfaceAlt)
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(vertical = Space.md),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label, style = Type.title(),
            color = if (active) c.textOnAccent else c.textSecondary,
        )
    }
}

// Total in / out / net for the open shift — In / Out as lighter stats above a
// tinted-teal Net block (the hero figure tellers look at, mirroring the cart's
// grand-total panel). Matches Flutter's `_SummaryStrip` projected onto the bold
// money language.
@Composable
private fun CashSummaryStrip(totalIn: Long, totalOut: Long, net: Long, currency: String) {
    val c = madarColors()
    MadarCard(spacing = Space.md) {
        Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            Stat(Modifier.weight(1f), t("cash.total_in"), "+ " + Money.format(totalIn, currency), c.success)
            Stat(Modifier.weight(1f), t("cash.total_out"), "− " + Money.format(totalOut, currency), c.danger)
        }
        // Net — the running figure for the shift, in the signature tinted-teal block.
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
                .padding(horizontal = Space.md, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t("cash.net"), style = Type.money(14.sp, FontWeight.Bold), color = c.accent)
            Box(Modifier.weight(1f))
            Text(
                (if (net < 0) "−" else "") + Money.format(abs(net), currency),
                style = Type.moneyLg(),
                color = if (net < 0) c.danger else c.accent,
            )
        }
    }
}

@Composable
private fun Stat(modifier: Modifier, label: String, value: String, tone: Color) {
    val c = madarColors()
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
        Text(
            label.uppercase(), style = Type.labelSm(), color = c.textMuted,
            letterSpacing = Motion.trackingSp.sp, maxLines = 1,
        )
        Text(value, style = Type.money(16.sp), color = tone, maxLines = 1)
    }
}

@Composable
private fun MovementsList(movements: List<CashMovementView>, currency: String) {
    val c = madarColors()
    SectionHeader(t("cash.history"))
    if (movements.isEmpty()) {
        Box(Modifier.fillMaxWidth().padding(vertical = Space.lg), contentAlignment = Alignment.Center) {
            Text(t("cash.empty"), style = Type.bodySm(), color = c.textMuted)
        }
    } else {
        // One card, rows separated by hairlines (matches Flutter's single
        // SurfaceCard(radius: AppRadius.lg) with a Divider between rows).
        MadarCard(padding = PaddingValues(0.dp), spacing = 0.dp) {
            movements.forEachIndexed { index, m ->
                if (index > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
                MovementRow(m, currency)
            }
        }
    }
}

@Composable
private fun MovementRow(m: CashMovementView, currency: String) {
    val c = madarColors()
    val positive = m.amountMinor >= 0L
    val tone = if (positive) c.success else c.danger
    val toneBg = if (positive) c.successBg else c.dangerBg
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(
            Modifier.size(38.dp).clip(CircleShape).background(toneBg),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon(if (positive) "arrow.down.left" else "arrow.up.right", tint = tone, size = IconSize.lg)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                m.note.ifEmpty { if (positive) t("cash.in") else t("cash.out") },
                style = Type.title(), color = c.textPrimary, maxLines = 1,
            )
            Text(m.movedByName, style = Type.bodySm(), color = c.textSecondary, maxLines = 1)
        }
        Text(
            "${if (positive) "+" else "−"} ${Money.format(abs(m.amountMinor), currency)}",
            style = Type.money(), color = tone,
        )
    }
}

// ── Past shifts ────────────────────────────────────────────────────────────────────

// Fixed wide-table column widths (Flutter `_Cols`: statusW = 10+16, declaredW, chevW).
private val ShiftStatusW = 26.dp
private val ShiftDeclaredW = 110.dp
private val ShiftChevW = 44.dp

/**
 * Flutter's `_withLocalOpenShift`: prepend the locally-opened-but-unsynced shift
 * (`model.shift`) to the top of the page if it isn't already present, so the live
 * shift always shows. `model.shift` is a `ShiftView`, projected onto a
 * `ShiftSummaryView` for the table.
 */
private fun shiftsWithLocalOpen(page: List<ShiftSummaryView>, live: ShiftView?): List<ShiftSummaryView> {
    if (live == null || !live.isOpen || page.any { it.id == live.id }) return page
    val pinned = ShiftSummaryView(
        id = live.id,
        branchName = null,
        tellerName = live.tellerName,
        openedAt = live.openedAt,
        closedAt = null,
        openingCashMinor = live.openingCashMinor,
        closingDeclaredMinor = null,
        closingSystemMinor = null,
        discrepancyMinor = null,
        status = live.status,
        isOpen = live.isOpen,
    )
    return listOf(pinned) + page
}

@Composable
fun ShiftHistoryScreen(model: AppModel) {
    val c = madarColors()
    val currency = model.session?.currencyCode ?: ""
    var expandedId by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { model.loadShiftHistory() }
    LaunchedEffect(expandedId) { expandedId?.let { model.loadOrdersForShift(it) } }

    val shifts = shiftsWithLocalOpen(model.shiftHistory, model.shift)

    Column(Modifier.fillMaxSize().background(c.bg)) {
        Box(Modifier.screenHeaderBar()) {
            ScreenHeader(t("shifts.title"), onBack = { model.showShiftHistory = false })
        }

        if (shifts.isEmpty()) {
            EmptyState("clock.arrow.circlepath", t("shifts.empty"))
        } else {
            // Width-driven, matching Flutter's `compact = maxWidth < 680`.
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val wide = maxWidth >= Responsive.wideTable
                Column(
                    Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    // Wide: header + rows live in one card (Flutter's single
                    // SurfaceCard); narrow keeps per-row cards.
                    val rowsModifier = Modifier.widthIn(max = 880.dp).fillMaxWidth()
                    if (wide) {
                        MadarCard(rowsModifier, padding = PaddingValues(0.dp), spacing = 0.dp) {
                            ColumnHeader()
                            shifts.forEachIndexed { index, s ->
                                ShiftRow(
                                    model, s, currency, wide = true,
                                    odd = index % 2 != 0,
                                    expanded = expandedId == s.id,
                                    onToggle = { expandedId = if (expandedId == s.id) null else s.id },
                                )
                            }
                        }
                    } else {
                        Column(
                            rowsModifier,
                            verticalArrangement = Arrangement.spacedBy(Space.sm),
                        ) {
                            shifts.forEachIndexed { index, s ->
                                ShiftRow(
                                    model, s, currency, wide = false,
                                    odd = index % 2 != 0,
                                    expanded = expandedId == s.id,
                                    onToggle = { expandedId = if (expandedId == s.id) null else s.id },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

// Flutter `_Cols`: [status dot 26][Teller flex2][Opened flex2][Closed flex2]
// [Declared 110 trailing][chevron 44]. Header omits the status-dot label (blank)
// and end-aligns Declared. 42-tall, surfaceAlt fill, bottom hairline.
@Composable
private fun ColumnHeader() {
    val c = madarColors()
    Box(Modifier.fillMaxWidth().background(c.surfaceAlt)) {
        Row(
            Modifier.fillMaxWidth().height(42.dp).padding(horizontal = Space.lg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            Spacer(Modifier.width(ShiftStatusW))
            HeaderCell(t("shift.teller"), Modifier.weight(1f))
            HeaderCell(t("shift.opened_at"), Modifier.weight(1f))
            HeaderCell(t("shifts.closed"), Modifier.weight(1f))
            HeaderCell(t("shifts.declared"), Modifier.width(ShiftDeclaredW), alignEnd = true)
            Spacer(Modifier.width(ShiftChevW))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight).align(Alignment.BottomStart))
    }
}

@Composable
private fun HeaderCell(label: String, modifier: Modifier, alignEnd: Boolean = false) {
    val c = madarColors()
    Box(modifier, contentAlignment = if (alignEnd) Alignment.CenterEnd else Alignment.CenterStart) {
        Text(label.uppercase(), style = Type.labelSm(), color = c.textMuted, maxLines = 1)
    }
}

@Composable
private fun ShiftRow(
    model: AppModel,
    s: ShiftSummaryView,
    currency: String,
    wide: Boolean,
    odd: Boolean,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var printing by remember { mutableStateOf(false) }

    // Wide rows alternate surface / surfaceAlt; expanded uses surfaceAlt as the hover
    // overlay. Narrow cards keep their solid surface.
    val rowBackground = when {
        !wide -> c.surface
        expanded -> c.surfaceAlt
        odd -> c.surfaceAlt
        else -> c.surface
    }
    // Status-dot color (Flutter `_statusColor`): open→success, force_closed→danger,
    // closed/other→muted.
    val statusColor = when (s.status) {
        "open" -> c.success
        "force_closed" -> c.danger
        else -> c.textMuted
    }

    val container = Modifier.fillMaxWidth().background(rowBackground).let {
        if (wide) it else it.clip(RoundedCornerShape(Radii.md)).border(1.dp, c.borderLight, RoundedCornerShape(Radii.md))
    }
    Column(container) {
        if (wide) {
            // A single table row — [status dot 26][Teller flex2][Opened flex2]
            // [Closed flex2][Declared 110 →][chevron 44]. Row height 56.
            Row(
                Modifier.fillMaxWidth().height(56.dp).clickable { onToggle() }.padding(horizontal = Space.lg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Box(Modifier.width(ShiftStatusW), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(8.dp).clip(CircleShape).background(statusColor))
                }
                Text(
                    s.tellerName ?: "—", style = Type.title(), color = c.textPrimary,
                    maxLines = 1, modifier = Modifier.weight(1f),
                )
                Text(
                    model.fmtDateTime(s.openedAt), style = Type.bodySm(), color = c.textSecondary,
                    maxLines = 1, modifier = Modifier.weight(1f),
                )
                Text(
                    s.closedAt?.let { model.fmtDateTime(it) } ?: "—",
                    style = Type.bodySm(),
                    color = if (s.closedAt == null) c.textMuted else c.textSecondary,
                    maxLines = 1, modifier = Modifier.weight(1f),
                )
                Box(Modifier.width(ShiftDeclaredW), contentAlignment = Alignment.CenterEnd) {
                    Text(
                        s.closingDeclaredMinor?.let { Money.format(it, currency) } ?: "—",
                        style = Type.money(),
                        color = if (s.closingDeclaredMinor == null) c.textMuted else c.textPrimary,
                        maxLines = 1,
                    )
                }
                Box(Modifier.width(ShiftChevW), contentAlignment = Alignment.Center) {
                    MadarIcon(if (expanded) "chevron.down" else "chevron.right", tint = c.textMuted, size = IconSize.sm)
                }
            }
        } else {
            // Narrow: a card.
            Column(
                Modifier.fillMaxWidth().clickable { onToggle() }.padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(model.fmtDateShort(s.openedAt), style = Type.title(), color = c.textPrimary)
                    Box(Modifier.weight(1f))
                    StatusChip(if (s.isOpen) t("shifts.open_now") else t("shifts.closed"), if (s.isOpen) ChipTone.SUCCESS else ChipTone.NEUTRAL)
                    MadarIcon(if (expanded) "chevron.down" else "chevron.right", tint = c.textMuted, size = IconSize.sm)
                }
                MetricRow(t("shifts.opening"), Money.format(s.openingCashMinor, currency))
                s.closingDeclaredMinor?.let { MetricRow(t("shifts.declared"), Money.format(it, currency)) }
                s.discrepancyMinor?.takeIf { it != 0L }?.let { disc ->
                    MetricRow(t("shifts.discrepancy"), "${if (disc > 0L) "+" else "−"}${Money.format(abs(disc), currency)}", tone = ChipTone.DANGER)
                }
            }
        }
        if (expanded) {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = Space.md).padding(bottom = Space.md),
                verticalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
                Row(Modifier.fillMaxWidth().padding(top = Space.sm), verticalAlignment = Alignment.CenterVertically) {
                    Text(t("shifts.orders").uppercase(), style = Type.label(), color = c.textMuted)
                    Box(Modifier.weight(1f))
                    Row(
                        Modifier.clickable(enabled = !printing) {
                            printing = true; scope.launch { model.openShiftReportPreviewFor(s.id); printing = false }
                        },
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(5.dp),
                    ) {
                        if (printing) CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp, modifier = Modifier.size(14.dp))
                        else MadarIcon("printer", tint = c.accent, size = IconSize.sm)
                        Text(t("shift.print_report"), style = Type.label(), color = c.accent)
                    }
                }
                val orders = model.shiftOrders[s.id]
                when {
                    model.loadingShiftOrders.contains(s.id) ->
                        Box(Modifier.fillMaxWidth().padding(vertical = Space.sm), contentAlignment = Alignment.Center) { CircularProgressIndicator(color = c.textMuted, strokeWidth = 2.dp, modifier = Modifier.size(18.dp)) }
                    orders.isNullOrEmpty() ->
                        Text(t("shifts.no_orders"), style = Type.bodySm(), color = c.textMuted, modifier = Modifier.padding(vertical = Space.sm))
                    else -> orders.forEach { o -> ShiftOrderRow(model, o, currency) }
                }
            }
        }
        if (wide) Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
    }
}

@Composable
private fun ShiftOrderRow(model: AppModel, o: OrderSummaryView, currency: String) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    // Tap a past order to preview its receipt (and print from the preview) — same
    // shared ReceiptPaper sheet as the Order History reprint.
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.xs)).background(c.surfaceAlt)
            .clickable { scope.launch { model.openOrderReceiptPreview(o.id) } }
            .padding(horizontal = Space.sm, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text(o.orderNumber?.let { "#$it" } ?: t("history.order"), style = Type.labelSm(), color = c.textPrimary)
        Text(model.fmtTime(o.createdAt), style = Type.labelSm(), color = c.textMuted)
        if (o.status == "voided") StatusChip(t("history.voided"), ChipTone.DANGER)
        Box(Modifier.weight(1f))
        Text(o.paymentLabel, style = Type.labelSm(), color = c.textMuted)
        Text(Money.format(o.totalMinor, currency), style = Type.money(12.sp, FontWeight.Bold), color = c.textPrimary)
        MadarIcon("printer", tint = c.accent, size = IconSize.sm)
    }
}
