package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Switch
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.OrderDetailLineView
import app.madar.core.OrderDetailView
import app.madar.core.OrderSummaryView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.SkeletonList
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.MadarButton
import app.madar.ui.MadarColors
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarTextField
import app.madar.ui.backGlyph
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Elevation
import app.madar.ui.elevation
import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.fillMaxHeight

// Order history — the current shift's orders: still-queued sales (Queued/Failed
// chip) plus the server's synced orders. Responsive: a sortable data TABLE at
// width ≥ 680 (mirroring the Flutter `_OrderTable`), stacked expandable CARDS
// below it. Tap a row to expand line detail (totals + Print + Void). Full shift
// stays in memory; only `visibleLimit` rows paint (client-side "show more").
// Mirror of the SwiftUI OrderHistoryView + VoidSheet.

// ── Sort model ────────────────────────────────────────────────────────────────
// The five sortable columns. Only `#`/number ascends by default; everything else
// descends (newest / biggest first).
private enum class OrderSortCol(val defaultAscending: Boolean) {
    NUMBER(true), PAYMENT(false), TIME(false), TELLER(false), AMOUNT(false),
}

// One sync-status filter axis value.
private enum class SyncFilter { ALL, SYNCED, PENDING, VOIDED }

private fun SyncFilter.matches(o: OrderSummaryView): Boolean = when (this) {
    SyncFilter.ALL -> true
    SyncFilter.SYNCED -> !o.queued && o.status != "voided"
    SyncFilter.PENDING -> o.queued
    SyncFilter.VOIDED -> o.status == "voided"
}

// One order-origin filter axis value.
private enum class TypeFilter { ALL, DINE_IN, DELIVERY }

private fun TypeFilter.matches(o: OrderSummaryView): Boolean = when (this) {
    TypeFilter.ALL -> true
    TypeFilter.DINE_IN -> o.orderType != "delivery"
    TypeFilter.DELIVERY -> o.orderType == "delivery"
}

private const val K_TABLE_BREAKPOINT = 680
private const val K_ORDER_PAGE_SIZE = 20

@Composable
fun OrderHistoryScreen(model: AppModel) {
    val c = madarColors()
    var expandedId by remember { mutableStateOf<String?>(null) }
    var voidTarget by remember { mutableStateOf<OrderSummaryView?>(null) }
    var search by remember { mutableStateOf("") }
    var syncFilter by remember { mutableStateOf(SyncFilter.ALL) }
    var typeFilter by remember { mutableStateOf(TypeFilter.ALL) }
    var sortCol by remember { mutableStateOf(OrderSortCol.NUMBER) }
    var sortAscending by remember { mutableStateOf(false) } // # defaults to DESC (newest first)
    var visibleLimit by remember { mutableStateOf(K_ORDER_PAGE_SIZE) }
    val currency = model.session?.currencyCode ?: ""
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { model.loadHistory() }

    fun resetPage() { visibleLimit = K_ORDER_PAGE_SIZE }

    fun matchesSearch(o: OrderSummaryView): Boolean {
        if (search.isBlank()) return true
        return (o.orderNumber?.toString() ?: "").contains(search) ||
            o.paymentLabel.contains(search, ignoreCase = true) ||
            (o.tellerName?.contains(search, ignoreCase = true) ?: false) ||
            (o.customerName?.contains(search, ignoreCase = true) ?: false)
    }

    fun <T : Comparable<T>> cmp(a: T, b: T): Int = if (sortAscending) a.compareTo(b) else b.compareTo(a)
    val comparator = Comparator<OrderSummaryView> { a, b ->
        when (sortCol) {
            OrderSortCol.NUMBER -> cmp(a.orderNumber ?: -1, b.orderNumber ?: -1)
            OrderSortCol.PAYMENT -> cmp(a.paymentLabel, b.paymentLabel)
            OrderSortCol.TIME -> cmp(a.createdAt, b.createdAt)
            OrderSortCol.TELLER -> cmp(a.tellerName ?: "", b.tellerName ?: "")
            OrderSortCol.AMOUNT -> cmp(a.totalMinor, b.totalMinor)
        }
    }

    // All rows passing search + both axes (AND), then sorted. Memoized so a row
    // expand / void-dialog toggle doesn't re-filter and re-sort the whole history.
    val filtered = remember(model.history, search, syncFilter, typeFilter, sortCol, sortAscending) {
        model.history
            .filter { matchesSearch(it) && syncFilter.matches(it) && typeFilter.matches(it) }
            .sortedWith(comparator)
    }
    // The slice actually painted (client-side pagination).
    val visible = filtered.take(visibleLimit)

    fun setSort(col: OrderSortCol) {
        if (sortCol == col) sortAscending = !sortAscending
        else { sortCol = col; sortAscending = col.defaultAscending }
        resetPage()
    }

    // Load the expanded row's lines (skip queued orders — they aren't on the
    // server yet). Mirrors the Swift `.task(id: expandedId)` detail loader.
    LaunchedEffect(expandedId) {
        val id = expandedId
        if (id != null) {
            val o = filtered.firstOrNull { it.id == id }
            if (o != null && !o.queued) model.loadOrderDetail(id)
        }
    }

    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().background(c.bg)) {
            // ── Header ────────────────────────────────────────────────────────
            Column(Modifier.fillMaxWidth().background(c.surface)) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.md),
                ) {
                    MadarIcon("chevron.backward", tint = c.textPrimary, size = 17.dp, modifier = Modifier.clickable { model.showHistory = false })
                    Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
                        Text(t("history.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 17.sp)
                        if (model.shift != null) {
                            Text(t("history.current_shift"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp)
                        }
                    }
                    Box(Modifier.weight(1f))
                    if (model.isLoadingHistory && model.history.isNotEmpty()) {
                        CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp, modifier = Modifier.size(18.dp))
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            // ── Filter bar (search + two filter-chip rows with counts) ─────────
            if (model.history.isNotEmpty()) {
                Column(
                    Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.sm),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    MadarTextField(value = search, onValueChange = { search = it; resetPage() }, placeholder = t("history.search"), icon = "magnifyingglass")
                    // Type axis (origin) — counts reflect search ∩ THIS chip's type rule.
                    Row(
                        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(Space.sm),
                    ) {
                        fun typeCount(f: TypeFilter) = model.history.count { matchesSearch(it) && f.matches(it) }
                        HistoryChip("slider.horizontal.3", "${t("history.type.all")} · ${typeCount(TypeFilter.ALL)}", typeFilter == TypeFilter.ALL, ChipTone.ACCENT) { typeFilter = TypeFilter.ALL; resetPage() }
                        HistoryChip("fork.knife", "${t("history.type.dine_in")} · ${typeCount(TypeFilter.DINE_IN)}", typeFilter == TypeFilter.DINE_IN, ChipTone.ACCENT) { typeFilter = TypeFilter.DINE_IN; resetPage() }
                        HistoryChip("shippingbox", "${t("history.type.delivery")} · ${typeCount(TypeFilter.DELIVERY)}", typeFilter == TypeFilter.DELIVERY, ChipTone.ACCENT) { typeFilter = TypeFilter.DELIVERY; resetPage() }
                    }
                    // Sync axis — counts reflect search ∩ type ∩ THIS chip's sync rule.
                    Row(
                        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(Space.sm),
                    ) {
                        fun syncCount(f: SyncFilter) = model.history.count { matchesSearch(it) && typeFilter.matches(it) && f.matches(it) }
                        HistoryChip("list.bullet", "${t("order.all")} · ${syncCount(SyncFilter.ALL)}", syncFilter == SyncFilter.ALL, ChipTone.ACCENT) { syncFilter = SyncFilter.ALL; resetPage() }
                        HistoryChip("checkmark.icloud", "${t("history.synced")} · ${syncCount(SyncFilter.SYNCED)}", syncFilter == SyncFilter.SYNCED, ChipTone.SUCCESS) { syncFilter = SyncFilter.SYNCED; resetPage() }
                        HistoryChip("icloud.and.arrow.up", "${t("history.queued")} · ${syncCount(SyncFilter.PENDING)}", syncFilter == SyncFilter.PENDING, ChipTone.WARNING) { syncFilter = SyncFilter.PENDING; resetPage() }
                        HistoryChip("xmark.circle", "${t("history.voided")} · ${syncCount(SyncFilter.VOIDED)}", syncFilter == SyncFilter.VOIDED, ChipTone.DANGER) { syncFilter = SyncFilter.VOIDED; resetPage() }
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
            }

            // ── Content ───────────────────────────────────────────────────────
            if (model.isLoadingHistory && model.history.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) { SkeletonList() }
            } else if (filtered.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                        MadarIcon(if (model.history.isEmpty()) "tray" else "line.3.horizontal.decrease.circle", tint = c.textMuted, size = 40.dp)
                        Text(if (model.history.isEmpty()) t("history.empty") else t("history.no_match"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 14.sp)
                    }
                }
            } else {
                BoxWithConstraints(Modifier.fillMaxSize()) {
                    val wide = maxWidth.value >= K_TABLE_BREAKPOINT
                    LazyColumn(
                        Modifier.widthIn(max = 960.dp).fillMaxWidth().fillMaxHeight().align(Alignment.TopCenter),
                        contentPadding = PaddingValues(Space.lg),
                        verticalArrangement = Arrangement.spacedBy(Space.lg),
                    ) {
                        item { StatsHeader(model, currency) }
                        if (wide) {
                            item {
                                OrderTable(
                                    model, visible, currency, expandedId, sortCol, sortAscending,
                                    onSort = { setSort(it) },
                                    onToggle = { id -> expandedId = if (expandedId == id) null else id },
                                    onPrint = { id -> scope.launch { model.openOrderReceiptPreview(id) } },
                                    onVoid = { voidTarget = it },
                                )
                            }
                        } else {
                            itemsIndexed(visible, key = { _, it -> it.id }) { _, item ->
                                OrderCard(
                                    model, item, currency, expandedId == item.id,
                                    onToggle = { expandedId = if (expandedId == item.id) null else item.id },
                                    onPrint = { scope.launch { model.openOrderReceiptPreview(item.id) } },
                                    onVoid = { voidTarget = item },
                                )
                            }
                        }
                        item { ShowMoreFooter(filtered.size, visible.size) { visibleLimit += K_ORDER_PAGE_SIZE } }
                    }
                }
            }
        }

        voidTarget?.let { VoidOverlay(model, it) { voidTarget = null } }
    }
}

// ── Stats header ──────────────────────────────────────────────────────────────
// `[orders count] | [Total (success)] [· one chip per payment method]`. Prefers
// the live shift report; folds over local (non-voided) history otherwise.
private data class PaymentBreakdown(val label: String, val amount: Long, val pct: Int, val tone: ChipTone)

@Composable
private fun StatsHeader(model: AppModel, currency: String) {
    val c = madarColors()
    val nonVoided = model.history.filter { it.status != "voided" }
    val total = model.shiftReport?.netPaymentsMinor ?: nonVoided.sumOf { it.totalMinor }
    val denom = maxOf(total, 1L)

    val breakdown: List<PaymentBreakdown> = run {
        val lines = model.shiftReport?.paymentLines
        if (lines != null && lines.isNotEmpty()) {
            lines.map { PaymentBreakdown(it.method, it.totalMinor, ((it.totalMinor.toDouble() / denom) * 100).toInt(), if (it.isCash) ChipTone.SUCCESS else ChipTone.INFO) }
        } else {
            val sums = LinkedHashMap<String, Long>()
            for (o in model.history) if (o.status != "voided") sums[o.paymentLabel] = (sums[o.paymentLabel] ?: 0L) + o.totalMinor
            sums.map { (label, amt) -> PaymentBreakdown(label, amt, ((amt.toDouble() / denom) * 100).toInt(), if (label.contains("cash", ignoreCase = true)) ChipTone.SUCCESS else ChipTone.INFO) }
        }
    }

    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md))
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = Space.lg, vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        StatCell(t("history.stat.orders"), nonVoided.size.toString(), c.textPrimary)
        Box(Modifier.width(1.dp).height(28.dp).background(c.border))
        StatCell(t("order.total"), Money.format(total, currency), c.success)
        breakdown.forEach { b ->
            StatusChip("${b.label} · ${Money.format(b.amount, currency)} · ${b.pct}%", b.tone)
        }
    }
}

@Composable
private fun StatCell(label: String, value: String, color: Color) {
    val c = madarColors()
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(label.uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
        Text(value, color = color, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 16.sp)
    }
}

// ── Wide TABLE ────────────────────────────────────────────────────────────────
@Composable
private fun OrderTable(
    model: AppModel,
    visible: List<OrderSummaryView>,
    currency: String,
    expandedId: String?,
    sortCol: OrderSortCol,
    sortAscending: Boolean,
    onSort: (OrderSortCol) -> Unit,
    onToggle: (String) -> Unit,
    onPrint: (String) -> Unit,
    onVoid: (OrderSummaryView) -> Unit,
) {
    val c = madarColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md)),
    ) {
        // Header
        Row(
            Modifier.fillMaxWidth().background(c.surfaceAlt).padding(horizontal = Space.md, vertical = Space.sm),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            HeaderCell("#", OrderSortCol.NUMBER, sortCol, sortAscending, onSort, Modifier.width(104.dp), trailing = false)
            HeaderCell(t("order.payment"), OrderSortCol.PAYMENT, sortCol, sortAscending, onSort, Modifier.weight(1f), trailing = false)
            HeaderCell(t("history.col.time"), OrderSortCol.TIME, sortCol, sortAscending, onSort, Modifier.weight(1f), trailing = false)
            HeaderCell(t("history.col.teller"), OrderSortCol.TELLER, sortCol, sortAscending, onSort, Modifier.weight(1f), trailing = false)
            HeaderCell(t("history.col.amount"), OrderSortCol.AMOUNT, sortCol, sortAscending, onSort, Modifier.width(110.dp), trailing = true)
            Spacer(Modifier.width(44.dp))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        visible.forEachIndexed { idx, item ->
            TableRow(
                model, item, currency, zebra = idx % 2 == 1, expanded = expandedId == item.id,
                onToggle = { onToggle(item.id) }, onPrint = { onPrint(item.id) }, onVoid = { onVoid(item) },
            )
            if (idx < visible.size - 1) Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
        }
    }
}

@Composable
private fun HeaderCell(
    label: String,
    col: OrderSortCol,
    sortCol: OrderSortCol,
    sortAscending: Boolean,
    onSort: (OrderSortCol) -> Unit,
    modifier: Modifier,
    trailing: Boolean,
) {
    val c = madarColors()
    val active = sortCol == col
    val fg = if (active) c.accent else c.textMuted
    Row(
        modifier.clickable { onSort(col) },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = if (trailing) Arrangement.spacedBy(3.dp, Alignment.End) else Arrangement.spacedBy(3.dp, Alignment.Start),
    ) {
        if (trailing && active) MadarIcon(if (sortAscending) "arrow.up" else "arrow.down", tint = fg, size = 9.dp)
        Text(label.uppercase(), color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
        if (!trailing && active) MadarIcon(if (sortAscending) "arrow.up" else "arrow.down", tint = fg, size = 9.dp)
    }
}

@Composable
private fun TableRow(
    model: AppModel,
    item: OrderSummaryView,
    currency: String,
    zebra: Boolean,
    expanded: Boolean,
    onToggle: () -> Unit,
    onPrint: () -> Unit,
    onVoid: () -> Unit,
) {
    val c = madarColors()
    val voided = item.status == "voided"
    val loadingDetail = expanded && !item.queued && model.orderDetail?.id != item.id
    val rowBg = if (expanded) c.navyBg else if (zebra) c.surfaceAlt else Color.Transparent
    Column(Modifier.fillMaxWidth().background(rowBg)) {
        Row(
            Modifier.fillMaxWidth().clickable { onToggle() }
                .padding(horizontal = Space.md).heightIn(56.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            // # cell — queued cloud icon, else number (+ optional ref)
            Box(Modifier.width(104.dp)) {
                if (item.queued) {
                    MadarIcon("icloud.and.arrow.up", tint = c.warning, size = IconSize.md)
                } else {
                    Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
                        Text(item.orderNumber?.let { "#$it" } ?: t("history.order"), color = c.navy, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                        item.orderRef?.let { Text(it, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 9.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                    }
                }
            }
            PaymentCell(item, voided, Modifier.weight(1f))
            Text(model.fmtTime(item.createdAt), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp, modifier = Modifier.weight(1f))
            TellerCell(item, 12.sp, Modifier.weight(1f))
            Text(
                Money.format(item.totalMinor, currency),
                color = if (voided) c.textMuted else c.textPrimary,
                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 14.sp,
                textDecoration = if (voided) TextDecoration.LineThrough else null,
                modifier = Modifier.width(110.dp),
            )
            Box(Modifier.width(44.dp), contentAlignment = Alignment.Center) {
                if (loadingDetail) CircularProgressIndicator(color = c.textMuted, strokeWidth = 2.dp, modifier = Modifier.size(16.dp))
                else Text(if (expanded) "▲" else "▼", color = c.textMuted, fontSize = 13.sp)
            }
        }
        if (expanded) {
            OrderDetailPanel(model, item, currency, onPrint, onVoid, Modifier.padding(horizontal = Space.md).padding(bottom = Space.md))
        }
    }
}

@Composable
private fun PaymentCell(item: OrderSummaryView, voided: Boolean, modifier: Modifier) {
    val c = madarColors()
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        PaymentBadge(item.paymentLabel, voided)
        when {
            voided -> StatusChip(t("history.voided"), ChipTone.DANGER)
            item.status == "failed" -> StatusChip(t("history.failed"), ChipTone.DANGER)
            item.queued -> StatusChip(t("history.queued"), ChipTone.WARNING, icon = "arrow.triangle.2.circlepath")
        }
        item.customerName?.let { Text(it, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    }
}

@Composable
private fun TellerCell(item: OrderSummaryView, fontSize: androidx.compose.ui.unit.TextUnit, modifier: Modifier) {
    val c = madarColors()
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        MadarIcon("person", tint = c.textMuted, size = IconSize.xs)
        Text(item.tellerName ?: "—", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = fontSize, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

// ── Narrow CARD ───────────────────────────────────────────────────────────────
@Composable
private fun OrderCard(
    model: AppModel,
    item: OrderSummaryView,
    currency: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    onPrint: () -> Unit,
    onVoid: () -> Unit,
) {
    val c = madarColors()
    val voided = item.status == "voided"
    val loadingDetail = expanded && !item.queued && model.orderDetail?.id != item.id
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md))
            .background(if (expanded) c.navyBg else c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md))
            .padding(horizontal = Space.lg, vertical = Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(Modifier.fillMaxWidth().clickable { onToggle() }, verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (item.queued) MadarIcon("icloud.and.arrow.up", tint = c.warning, size = IconSize.sm)
                    Text(item.orderNumber?.let { "#$it" } ?: t("history.order"), color = c.navy, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    Text(model.fmtTime(item.createdAt), color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp)
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    PaymentBadge(item.paymentLabel, voided)
                    when {
                        voided -> StatusChip(t("history.voided"), ChipTone.DANGER)
                        item.status == "failed" -> StatusChip(t("history.failed"), ChipTone.DANGER)
                        item.queued -> StatusChip(t("history.queued"), ChipTone.WARNING)
                    }
                }
                item.customerName?.let { Text(it, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                TellerCell(item, 11.sp, Modifier)
            }
            Spacer(Modifier.width(Space.sm))
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    Money.format(item.totalMinor, currency),
                    color = if (voided) c.textMuted else c.textPrimary,
                    fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp,
                    textDecoration = if (voided) TextDecoration.LineThrough else null,
                )
                if (loadingDetail) CircularProgressIndicator(color = c.textMuted, strokeWidth = 2.dp, modifier = Modifier.size(16.dp))
                else Text(if (expanded) "▲" else "▼", color = c.textMuted, fontSize = 12.sp)
            }
        }
        if (expanded) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            OrderDetailPanel(model, item, currency, onPrint, onVoid, Modifier)
        }
    }
}

// ── Shared expanded detail (line items + totals + Print/Void) ─────────────────
@Composable
private fun OrderDetailPanel(
    model: AppModel,
    item: OrderSummaryView,
    currency: String,
    onPrint: () -> Unit,
    onVoid: () -> Unit,
    modifier: Modifier,
) {
    val c = madarColors()
    val canVoid = !item.queued && item.status != "voided"
    val canPrint = !item.queued && item.status != "voided"
    val detail = model.orderDetail?.takeIf { it.id == item.id }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        if (detail != null) {
            detail.lines.forEach { line -> LineRow(line, currency) }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
            DetailRow(t("order.subtotal"), Money.format(detail.subtotalMinor, currency))
            if (detail.discountMinor > 0) DetailRow(t("order.discount"), "− " + Money.format(detail.discountMinor, currency), c.success)
            DetailRow(t("order.tax"), Money.format(detail.taxMinor, currency))
        } else {
            // Queued/offline order, or detail not yet loaded — summary totals.
            DetailRow(t("order.subtotal"), Money.format(item.subtotalMinor, currency))
            DetailRow(t("order.tax"), Money.format(item.taxMinor, currency))
        }
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            Text(item.paymentLabel, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
            Box(Modifier.weight(1f))
            if (canPrint) {
                Row(Modifier.clickable { onPrint() }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) { MadarIcon("printer", tint = c.accent, size = 12.dp); Text(t("receipt.print"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp) }
            }
            if (canVoid) {
                Row(Modifier.clickable { onVoid() }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) { MadarIcon("trash", tint = c.danger, size = 12.dp); Text(t("void.action"), color = c.danger, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp) }
            }
        }
    }
}

@Composable
private fun DetailRow(label: String, value: String, color: Color? = null) {
    val c = madarColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Text(value, color = color ?: c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

// One fetched order line — "qty× name" + its modifiers on the left, the line
// total on the right. Mirror of Swift lineRow.
@Composable
private fun LineRow(line: OrderDetailLineView, currency: String) {
    val c = madarColors()
    val mods = listOfNotNull(line.sizeLabel) + line.addons + line.optionals
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text("${line.qty}× ${line.name}", color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            if (mods.isNotEmpty()) Text(mods.joinToString(" · "), color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp, maxLines = 2)
        }
        Spacer(Modifier.width(Space.sm))
        Text(Money.format(line.lineTotalMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

// ── Payment badge ─────────────────────────────────────────────────────────────
// A colored payment pill (not a StatusChip): tinted bg @ ~14%, colored label.
// Voided → muted/surfaceAlt. Color is keyed off the label text.
@Composable
private fun PaymentBadge(label: String, voided: Boolean) {
    val c = madarColors()
    val tint = paymentTint(label, c)
    val fg = if (voided) c.textMuted else tint
    Text(
        label,
        color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp,
        modifier = Modifier.clip(CircleShape)
            .background(if (voided) c.surfaceAlt else tint.copy(alpha = 0.14f))
            .padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

private fun paymentTint(label: String, c: MadarColors): Color {
    val l = label.lowercase()
    return when {
        l.contains("cash") || l.contains("نقد") -> c.success
        l.contains("card") || l.contains("بطاق") -> Color(0xFF7C3AED)
        l.contains("mixed") || l.contains("مختلط") -> c.warning
        else -> c.navy
    }
}

// ── Pagination footer ─────────────────────────────────────────────────────────
@Composable
private fun ShowMoreFooter(filteredCount: Int, visibleCount: Int, onShowMore: () -> Unit) {
    val c = madarColors()
    val remaining = filteredCount - visibleCount
    if (remaining <= 0) return
    val count = minOf(K_ORDER_PAGE_SIZE, remaining)
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
            .clickable { onShowMore() }.padding(vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
    ) {
        MadarIcon("chevron.down", tint = c.accent, size = IconSize.xs)
        Text(t("history.show_more").replace("{count}", count.toString()), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

// ── Filter chip ───────────────────────────────────────────────────────────────
// Filled in its active tone, neutral when off. Mirrors the Swift `chip`.
@Composable
private fun HistoryChip(glyph: String, label: String, active: Boolean, tone: ChipTone, onClick: () -> Unit) {
    val c = madarColors()
    val fg = if (active) toneFg(tone, c) else c.textSecondary
    val bg = if (active) toneBg(tone, c) else c.surfaceAlt
    Row(
        Modifier.clip(CircleShape).background(bg)
            .border(1.dp, if (active) fg.copy(alpha = 0.25f) else Color.Transparent, CircleShape)
            .clickable { onClick() }.padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        MadarIcon(glyph, tint = fg, size = IconSize.xs)
        Text(label, color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
    }
}

private fun toneFg(tone: ChipTone, c: MadarColors): Color = when (tone) {
    ChipTone.INFO -> c.navy; ChipTone.ACCENT -> c.accent; ChipTone.SUCCESS -> c.success
    ChipTone.WARNING -> c.warning; ChipTone.DANGER -> c.danger; ChipTone.NEUTRAL -> c.textSecondary
}

private fun toneBg(tone: ChipTone, c: MadarColors): Color = when (tone) {
    ChipTone.INFO -> c.navyBg; ChipTone.ACCENT -> c.accentBg; ChipTone.SUCCESS -> c.successBg
    ChipTone.WARNING -> c.warningBg; ChipTone.DANGER -> c.dangerBg; ChipTone.NEUTRAL -> c.surfaceAlt
}

// ── Void overlay ──────────────────────────────────────────────────────────────
@Composable
private fun VoidOverlay(model: AppModel, order: OrderSummaryView, onDone: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var reason by remember { mutableStateOf("mistake") }
    var note by remember { mutableStateOf("") }
    var restock by remember { mutableStateOf(true) }
    val currency = model.session?.currencyCode ?: ""
    val reasons = listOf(
        "mistake" to "void.reason_mistake",
        "customer" to "void.reason_customer",
        "quality" to "void.reason_quality",
        "other" to "void.reason_other",
    )

    // System back dismisses the void sheet (not the whole history screen).
    BackHandlerCompat(enabled = true) { onDone() }
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onDone() },
        )
        Column(
            Modifier.align(Alignment.BottomCenter).widthIn(max = 520.dp).fillMaxWidth()
                .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}
                .verticalScroll(rememberScrollState()).padding(Space.xl),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(t("void.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 22.sp)
                Box(Modifier.weight(1f))
                MadarIcon("xmark", tint = c.textMuted, size = IconSize.lg, modifier = Modifier.clickable { onDone() })
            }
            Row(
                Modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.sm)).clip(RoundedCornerShape(Radii.sm)).background(c.surface)
                    .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm)).padding(Space.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(order.orderNumber?.let { "#$it" } ?: t("history.order"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                Box(Modifier.weight(1f))
                Text(Money.format(order.totalMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            }
            Text(t("void.reason"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
            reasons.forEach { (key, label) -> ReasonRow(t(label), reason == key) { reason = key } }
            MadarTextField(note, { note = it }, t("void.note"), enabled = !model.isBusy)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                Text(t("void.restock"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontSize = 14.sp, modifier = Modifier.weight(1f))
                Switch(checked = restock, onCheckedChange = { restock = it })
            }
            model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
            Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                MadarButton(t("void.cancel"), { onDone() }, modifier = Modifier.weight(1f), variant = BtnVariant.OUTLINE)
                MadarButton(
                    t("void.confirm"),
                    { scope.launch { if (model.voidOrder(order.id, reason, note, restock)) onDone() } },
                    modifier = Modifier.weight(1f), variant = BtnVariant.DANGER, loading = model.isBusy, icon = "trash",
                )
            }
        }
    }
}

@Composable
private fun ReasonRow(label: String, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm))
            .background(if (active) c.dangerBg else c.surface)
            .border(1.dp, if (active) c.danger.copy(alpha = 0.45f) else c.border, RoundedCornerShape(Radii.sm))
            .clickable { onClick() }.padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        MadarIcon(if (active) "largecircle.fill.circle" else "circle", tint = if (active) c.danger else c.textMuted, size = IconSize.lg)
        Text(label, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = if (active) FontWeight.Bold else FontWeight.Medium, fontSize = 14.sp)
    }
}
