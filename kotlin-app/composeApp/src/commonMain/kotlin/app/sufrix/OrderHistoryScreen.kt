package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
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
import app.sufrix.core.OrderDetailLineView
import app.sufrix.core.OrderDetailView
import app.sufrix.core.OrderSummaryView
import app.sufrix.ui.BtnVariant
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
import kotlinx.coroutines.launch

// Order history — the current shift's orders: still-queued sales (Queued/Failed
// chip) + the server's synced orders. Tap a row for totals; void a synced order.
// Mirror of the SwiftUI OrderHistoryView + VoidSheet.
@Composable
fun OrderHistoryScreen(model: AppModel) {
    val c = sufrixColors()
    var expandedId by remember { mutableStateOf<String?>(null) }
    var voidTarget by remember { mutableStateOf<OrderSummaryView?>(null) }
    var search by remember { mutableStateOf("") }
    var statusFilter by remember { mutableStateOf<String?>(null) } // null=all; completed|voided|queued
    val currency = model.session?.currencyCode ?: ""
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { model.loadHistory() }

    val filtered = model.history.filter { o ->
        val matchesSearch = search.isBlank() ||
            (o.orderNumber?.toString() ?: "").contains(search) ||
            o.paymentLabel.contains(search, ignoreCase = true)
        val matchesStatus = when (statusFilter) {
            null -> true
            "queued" -> o.queued
            else -> o.status == statusFilter
        }
        matchesSearch && matchesStatus
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
                    Text(backGlyph(), color = c.textPrimary, fontSize = 26.sp, modifier = Modifier.clickable { model.showHistory = false })
                    Text(t("history.title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
                    Box(Modifier.weight(1f))
                    if (model.isLoadingHistory && model.history.isNotEmpty()) {
                        CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp, modifier = Modifier.size(18.dp))
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            // ── Filter bar (search + status chips) ──────────────────────────────
            if (model.history.isNotEmpty()) {
                Column(
                    Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.sm),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    SufrixTextField(value = search, onValueChange = { search = it }, placeholder = t("history.search"))
                    Row(
                        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(Space.sm),
                    ) {
                        FilterChip(t("order.all"), statusFilter == null) { statusFilter = null }
                        FilterChip(t("history.completed"), statusFilter == "completed") { statusFilter = "completed" }
                        FilterChip(t("history.queued"), statusFilter == "queued") { statusFilter = "queued" }
                        FilterChip(t("history.voided"), statusFilter == "voided") { statusFilter = "voided" }
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
            }

            // ── Content ───────────────────────────────────────────────────────
            if (filtered.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    if (model.isLoadingHistory) {
                        CircularProgressIndicator(color = c.accent)
                    } else {
                        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                            Text("▦", color = c.textMuted, fontSize = 40.sp)
                            Text(t("history.empty"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
                        }
                    }
                }
            } else {
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(Space.lg),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    items(filtered, key = { it.id }) { item ->
                        HistoryRow(
                            item, currency, expandedId == item.id,
                            detail = model.orderDetail?.takeIf { it.id == item.id },
                            onToggle = { expandedId = if (expandedId == item.id) null else item.id },
                            onVoid = { voidTarget = item },
                            onReprint = { scope.launch { model.reprintOrder(item.id) } },
                        )
                    }
                }
            }
        }

        voidTarget?.let { VoidOverlay(model, it) { voidTarget = null } }
    }
}

@Composable
private fun HistoryRow(
    item: OrderSummaryView,
    currency: String,
    expanded: Boolean,
    detail: OrderDetailView?,
    onToggle: () -> Unit,
    onVoid: () -> Unit,
    onReprint: () -> Unit,
) {
    val c = sufrixColors()
    val canVoid = !item.queued && item.status != "voided"
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(Modifier.fillMaxWidth().clickable { onToggle() }, verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(item.orderNumber?.let { "#$it" } ?: t("history.order"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                Text(timeOf(item.createdAt), color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)
            }
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(Money.format(item.totalMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                StatusChipFor(item)
            }
        }
        if (expanded) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            if (detail != null && detail.id == item.id) {
                detail.lines.forEach { line -> LineRow(line, currency) }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
            }
            DetailRow(t("order.subtotal"), Money.format(item.subtotalMinor, currency))
            DetailRow(t("order.tax"), Money.format(item.taxMinor, currency))
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Text(item.paymentLabel, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
                Box(Modifier.weight(1f))
                if (canVoid) {
                    Text(
                        "⎙ ${t("receipt.print")}", color = c.accent, fontFamily = SufrixFont,
                        fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                        modifier = Modifier.clickable { onReprint() },
                    )
                    Text(
                        "✕ ${t("void.action")}", color = c.danger, fontFamily = SufrixFont,
                        fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                        modifier = Modifier.clickable { onVoid() },
                    )
                }
            }
        }
    }
}

@Composable
private fun StatusChipFor(item: OrderSummaryView) {
    when {
        item.status == "failed" -> StatusChip(t("history.failed"), ChipTone.DANGER)
        item.queued -> StatusChip(t("history.queued"), ChipTone.WARNING)
        item.status == "voided" -> StatusChip(t("history.voided"), ChipTone.DANGER)
    }
}

@Composable
private fun DetailRow(label: String, value: String) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
        Box(Modifier.weight(1f))
        Text(value, color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
    }
}

// One fetched order line — "qty× name" + its modifiers (size · addons ·
// optionals) on the left, the line total on the right. Mirror of Swift lineRow.
@Composable
private fun LineRow(line: OrderDetailLineView, currency: String) {
    val c = sufrixColors()
    val mods = (listOfNotNull(line.sizeLabel) + line.addons + line.optionals)
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text("${line.qty}× ${line.name}", color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 12.sp)
            if (mods.isNotEmpty()) {
                Text(mods.joinToString(" · "), color = c.textMuted, fontFamily = SufrixFont, fontSize = 10.sp, maxLines = 2)
            }
        }
        Text(Money.format(line.lineTotalMinor, currency), color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
    }
}

// ── Void overlay ────────────────────────────────────────────────────────────────
@Composable
private fun VoidOverlay(model: AppModel, order: OrderSummaryView, onDone: () -> Unit) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var reason by remember { mutableStateOf("mistake") }
    var note by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""
    val reasons = listOf(
        "mistake" to "void.reason_mistake",
        "customer" to "void.reason_customer",
        "quality" to "void.reason_quality",
        "other" to "void.reason_other",
    )

    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onDone() },
        )
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}
                .verticalScroll(rememberScrollState()).padding(Space.xl),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(t("void.title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 22.sp)
                Box(Modifier.weight(1f))
                Text("✕", color = c.textMuted, fontSize = 18.sp, modifier = Modifier.clickable { onDone() })
            }
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
                    .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(order.orderNumber?.let { "#$it" } ?: t("history.order"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                Box(Modifier.weight(1f))
                Text(Money.format(order.totalMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            }
            Text(t("void.reason"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
            reasons.forEach { (key, label) -> ReasonRow(t(label), reason == key) { reason = key } }
            SufrixTextField(note, { note = it }, t("void.note"), enabled = !model.isBusy)
            model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
            SufrixButton(
                t("void.confirm"),
                { scope.launch { if (model.voidOrder(order.id, reason, note)) onDone() } },
                variant = BtnVariant.DANGER, loading = model.isBusy,
            )
            SufrixButton(t("void.cancel"), { onDone() }, variant = BtnVariant.GHOST)
        }
    }
}

@Composable
private fun ReasonRow(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm))
            .background(if (active) c.accentBg else c.surface)
            .border(1.dp, if (active) c.accent else c.border, RoundedCornerShape(Radii.sm))
            .clickable { onClick() }.padding(vertical = 11.dp, horizontal = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Text(if (active) "●" else "○", color = if (active) c.accent else c.textMuted, fontSize = 14.sp)
        Text(label, color = c.textPrimary, fontFamily = SufrixFont, fontSize = 14.sp)
    }
}

/** rfc3339 → "HH:MM". */
private fun timeOf(rfc3339: String): String {
    val i = rfc3339.indexOf('T')
    return if (i >= 0) rfc3339.substring(i + 1).take(5) else rfc3339
}

@Composable
private fun FilterChip(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    Text(
        label,
        color = if (active) c.textOnAccent else c.textSecondary,
        fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
        modifier = Modifier.clip(CircleShape).background(if (active) c.accent else c.surfaceAlt)
            .clickable { onClick() }.padding(horizontal = 12.dp, vertical = 6.dp),
    )
}
