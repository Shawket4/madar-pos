@file:OptIn(ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.DeliveryOrderView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.SkeletonList
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.MadarButton
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarSheet
import app.madar.ui.MadarTextField
import app.madar.ui.SectionHeader
import app.madar.ui.SelectableChip
import app.madar.ui.SheetSize
import app.madar.ui.Type
import app.madar.ui.backGlyph
import app.madar.ui.madarColors
import app.madar.ui.t
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

// Delivery queue — the teller works a branch's live delivery orders: advance the
// lifecycle, bump prep time, cancel (with restock), and finalize into a real sale
// on the open shift. All logic in the core; this only renders + collects. Online,
// refreshes on open + a light poll. Mirror of the SwiftUI DeliveryView.
@Composable
// Delivery queue body — the "Delivery" tab of the unified Orders surface. No
// nav header of its own (the unified screen owns back + title + the tab bar);
// this is just the Active/All filter toolbar + accepting chips + the live list.
fun DeliveryBody(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    var finalizing by remember { mutableStateOf<DeliveryOrderView?>(null) }
    var cancelling by remember { mutableStateOf<DeliveryOrderView?>(null) }

    LaunchedEffect(Unit) { model.loadDeliveryOrders() }
    // SSE is primary now: delivery events arrive on the session-level subscription
    // and bump `deliveryTick` → reload. The slow poll below is just a safety net.
    LaunchedEffect(model.deliveryTick) { model.loadDeliveryOrders() }
    LaunchedEffect(Unit) {
        while (isActive) {
            delay(60_000)
            model.loadDeliveryOrders()
        }
    }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxSize()) {
            // Active/All filter toolbar (the unified header owns back + title).
            Column(Modifier.fillMaxWidth().background(c.surface)) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Spacer(Modifier.weight(1f))
                    SegToggle(model.deliveryActiveOnly) { active ->
                        model.deliveryActiveOnly = active
                        scope.launch { model.loadDeliveryOrders() }
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            model.deliverySettings?.let { s ->
                Row(
                    Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.sm),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    Text(t("delivery.accepting"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
                    AcceptingChip(t("delivery.in_mall"), "in_mall", s.inMallOverride, s.inMallEnabled, scope, model)
                    AcceptingChip(t("delivery.outside"), "outside", s.outsideOverride, s.outsideEnabled, scope, model)
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            model.error?.let {
                Box(Modifier.padding(Space.lg)) { NoticeBanner(it, ChipTone.WARNING, icon = "exclamationmark.circle") }
            }

            if (model.isLoadingDelivery && model.deliveryOrders.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) { SkeletonList() }
            } else if (model.deliveryOrders.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                        MadarIcon("bicycle", tint = c.textMuted, size = 40.dp)
                        Text(t("delivery.empty"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 14.sp)
                    }
                }
            } else {
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(Space.lg),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    items(model.deliveryOrders, key = { it.id }) { o ->
                        Box(Modifier.widthIn(max = 620.dp).fillMaxWidth()) {
                            DeliveryOrderCard(
                                model, o, currency,
                                onAdvance = { scope.launch { model.advanceDelivery(o) } },
                                onPrep = { scope.launch { model.addDeliveryPrep(o) } },
                                onFinalize = { finalizing = o },
                                onCancel = { cancelling = o },
                                onReject = { scope.launch { model.rejectDelivery(o) } },
                            )
                        }
                    }
                }
            }
        }

        // Finalize / cancel as branded bottom sheets (were centered scrim dialogs)
        // — one sheet idiom across the app, matching Swift's madarSheet.
        finalizing?.let { o ->
            MadarSheet(onDismiss = { finalizing = null }, size = SheetSize.HUG, maxWidth = 480.dp) { dismiss ->
                FinalizeSheetContent(model, o, currency, dismiss)
            }
        }
        cancelling?.let { o ->
            MadarSheet(onDismiss = { cancelling = null }, size = SheetSize.HUG, maxWidth = 480.dp) { dismiss ->
                CancelSheetContent(model, o, dismiss)
            }
        }
    }
}

@Composable
private fun AcceptingChip(label: String, channel: String, mode: String, enabled: Boolean, scope: kotlinx.coroutines.CoroutineScope, model: AppModel) {
    // Dashboard-disabled channels can't be opened; show them muted.
    val tone = if (!enabled) ChipTone.NEUTRAL else when (mode) {
        "closed" -> ChipTone.DANGER
        "open" -> ChipTone.SUCCESS
        else -> ChipTone.ACCENT
    }
    val modeLabel = t("delivery.mode_$mode")
    Box(
        Modifier.clickable(enabled = enabled && !model.isBusy) { scope.launch { model.cycleAccepting(channel, mode) } }
            .alpha(if (enabled) 1f else 0.5f),
    ) {
        StatusChip("$label: $modeLabel", tone)
    }
}

@Composable
private fun SegToggle(activeOnly: Boolean, onChange: (Boolean) -> Unit) {
    val c = madarColors()
    Row(
        Modifier.clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt).padding(2.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        @Composable
        fun seg(label: String, on: Boolean, value: Boolean) {
            Text(
                label, color = if (on) c.textOnAccent else c.textSecondary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                modifier = Modifier.clip(RoundedCornerShape(Radii.sm - 2.dp))
                    .background(if (on) c.accent else Color.Transparent)
                    .clickable { onChange(value) }.padding(horizontal = Space.md, vertical = 6.dp),
            )
        }
        seg(t("delivery.active"), activeOnly, true)
        seg(t("delivery.all"), !activeOnly, false)
    }
}

@Composable
private fun DeliveryOrderCard(
    model: AppModel,
    o: DeliveryOrderView,
    currency: String,
    onAdvance: () -> Unit,
    onPrep: () -> Unit,
    onFinalize: () -> Unit,
    onCancel: () -> Unit,
    onReject: () -> Unit,
) {
    val c = madarColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            StatusChip(t("delivery.status.${o.status}"), statusTone(o.status))
            StatusChip(t("delivery.${o.channel}"), ChipTone.NEUTRAL)
            Spacer(Modifier.weight(1f))
            o.orderRef?.let { Text(it, color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp) }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(o.customerName, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            Spacer(Modifier.weight(1f))
            Text(Money.format(o.totalMinor, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp)
        }
        Text(o.customerPhone, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
        o.address?.let { Text(it, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp, maxLines = 2) }
        // Customer delivery instructions ("leave at door", "call on arrival") —
        // fulfillment-critical text the core already carries but neither host
        // rendered. Warning-tinted so the dispatcher can't miss it.
        o.deliveryNotes?.takeIf { it.isNotBlank() }?.let { note ->
            Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
                MadarIcon("text.bubble", tint = c.warning, size = IconSize.sm)
                Text(note, color = c.warning, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 12.sp)
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text("${o.itemCount} ${t("delivery.items")}", color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 11.sp)
            if (o.deliveryFeeMinor > 0) {
                Text(
                    "· ${t("receipt.delivery_fee")} ${Money.format(o.deliveryFeeMinor, currency)}",
                    color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp,
                )
            }
        }

        if (!o.isTerminal) {
            Row(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalAlignment = Alignment.CenterVertically) {
                nextStatus(o.status)?.let { next ->
                    MadarButton(t("delivery.action.$next"), onAdvance, fullWidth = false, icon = "arrow.right.circle")
                }
                Spacer(Modifier.weight(1f))
                var menuOpen by remember { mutableStateOf(false) }
                Box {
                    MadarIcon("ellipsis.circle", tint = c.textSecondary, size = 22.dp,
                        modifier = Modifier.clickable { menuOpen = true })
                    DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                        DropdownMenuItem(
                            text = { Text(t("delivery.add_prep"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontSize = 14.sp) },
                            leadingIcon = { MadarIcon("clock", tint = c.textSecondary, size = IconSize.md) },
                            onClick = { menuOpen = false; onPrep() },
                        )
                        DropdownMenuItem(
                            text = { Text(t("delivery.finalize"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontSize = 14.sp) },
                            leadingIcon = { MadarIcon("checkmark.seal", tint = c.textSecondary, size = IconSize.md) },
                            onClick = { menuOpen = false; onFinalize() },
                        )
                        // Reject is the terminal "refuse incoming work" action — only
                        // a just-received order can be rejected (before any prep).
                        if (o.status == "received") {
                            DropdownMenuItem(
                                text = { Text(t("delivery.reject"), color = c.danger, fontFamily = LocalMadarFont.current, fontSize = 14.sp) },
                                leadingIcon = { MadarIcon("hand.raised", tint = c.danger, size = IconSize.md) },
                                onClick = { menuOpen = false; onReject() },
                            )
                        }
                        DropdownMenuItem(
                            text = { Text(t("delivery.cancel"), color = c.danger, fontFamily = LocalMadarFont.current, fontSize = 14.sp) },
                            leadingIcon = { MadarIcon("xmark.circle", tint = c.danger, size = IconSize.md) },
                            onClick = { menuOpen = false; onCancel() },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ColumnScope.FinalizeSheetContent(model: AppModel, o: DeliveryOrderView, currency: String, dismiss: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var method by remember { mutableStateOf(model.paymentMethods.firstOrNull { it.isCash }?.id ?: model.paymentMethods.firstOrNull()?.id) }
    Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
        Text(t("delivery.finalize"), style = Type.h2(), color = c.textPrimary)
        Text("${o.customerName} · ${Money.format(o.totalMinor, currency)}", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
        SectionHeader(t("delivery.finalize_pay"))
        FlowRow(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            model.paymentMethods.forEach { m ->
                SelectableChip(m.name, isSelected = m.id == method, onTap = { method = m.id })
            }
        }
        MadarButton(
            t("delivery.finalize"), {
                method?.let { id -> scope.launch { if (model.finalizeDelivery(o, id)) dismiss() } }
            },
            loading = model.isBusy, enabled = method != null, icon = "checkmark.seal",
        )
    }
}

@Composable
private fun ColumnScope.CancelSheetContent(model: AppModel, o: DeliveryOrderView, dismiss: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var reason by remember { mutableStateOf("") }
    var restock by remember { mutableStateOf(true) }
    Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
        Text(t("delivery.cancel"), style = Type.h2(), color = c.textPrimary)
        Text(o.customerName, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
        MadarTextField(reason, { reason = it }, t("delivery.cancel_reason"), icon = "text.bubble")
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text(t("delivery.restore_inventory"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontSize = 14.sp, modifier = Modifier.weight(1f))
            Switch(checked = restock, onCheckedChange = { restock = it })
        }
        MadarButton(
            t("delivery.cancel"), {
                scope.launch { if (model.cancelDelivery(o, reason.ifBlank { null }, restock)) dismiss() }
            },
            variant = BtnVariant.DANGER, loading = model.isBusy, icon = "xmark.circle",
        )
    }
}

private fun nextStatus(s: String): String? = when (s) {
    "received" -> "confirmed"
    "confirmed" -> "preparing"
    "preparing" -> "ready"
    "ready" -> "out_for_delivery"
    "out_for_delivery" -> "delivered"
    else -> null
}

private fun statusTone(s: String): ChipTone = when (s) {
    "received" -> ChipTone.INFO
    "confirmed", "out_for_delivery" -> ChipTone.ACCENT
    "preparing" -> ChipTone.WARNING
    "ready", "delivered" -> ChipTone.SUCCESS
    "cancelled", "rejected" -> ChipTone.DANGER
    else -> ChipTone.NEUTRAL
}
