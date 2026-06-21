package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.DeliveryOrderView
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.Radii
import app.sufrix.ui.SkeletonList
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.backGlyph
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

// Delivery queue — the teller works a branch's live delivery orders: advance the
// lifecycle, bump prep time, cancel (with restock), and finalize into a real sale
// on the open shift. All logic in the core; this only renders + collects. Online,
// refreshes on open + a light poll. Mirror of the SwiftUI DeliveryView.
@Composable
fun DeliveryScreen(model: AppModel) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    var finalizing by remember { mutableStateOf<DeliveryOrderView?>(null) }
    var cancelling by remember { mutableStateOf<DeliveryOrderView?>(null) }

    LaunchedEffect(Unit) { model.loadDeliveryOrders() }
    // Light poll while the queue is open (the rebuild's realtime stand-in).
    LaunchedEffect(Unit) {
        while (isActive) {
            delay(15_000)
            model.loadDeliveryOrders()
        }
    }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxSize()) {
            // Header + Active/All toggle.
            Column(Modifier.fillMaxWidth().background(c.surface)) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.md),
                ) {
                    Text(backGlyph(), color = c.textPrimary, fontSize = 26.sp, modifier = Modifier.clickable { model.showDelivery = false })
                    Text(t("delivery.queue"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
                    Spacer(Modifier.weight(1f))
                    SegToggle(model.deliveryActiveOnly) { active ->
                        model.deliveryActiveOnly = active
                        scope.launch { model.loadDeliveryOrders() }
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            model.error?.let { NoticeBanner(it, ChipTone.WARNING) }

            if (model.isLoadingDelivery && model.deliveryOrders.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) { SkeletonList() }
            } else if (model.deliveryOrders.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                        Text("🛵", fontSize = 40.sp)
                        Text(t("delivery.empty"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
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
                            )
                        }
                    }
                }
            }
        }

        finalizing?.let { o ->
            FinalizeDialog(model, o, currency, onDismiss = { finalizing = null })
        }
        cancelling?.let { o ->
            CancelDialog(model, o, onDismiss = { cancelling = null })
        }
    }
}

@Composable
private fun SegToggle(activeOnly: Boolean, onChange: (Boolean) -> Unit) {
    val c = sufrixColors()
    Row(
        Modifier.clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt).padding(2.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        @Composable
        fun seg(label: String, on: Boolean, value: Boolean) {
            Text(
                label, color = if (on) c.textOnAccent else c.textSecondary, fontFamily = SufrixFont,
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
) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            StatusChip(t("delivery.status.${o.status}"), statusTone(o.status))
            StatusChip(t("delivery.${o.channel}"), ChipTone.NEUTRAL)
            Spacer(Modifier.weight(1f))
            o.orderRef?.let { Text(it, color = c.textMuted, fontFamily = SufrixFont, fontSize = 12.sp) }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(o.customerName, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            Spacer(Modifier.weight(1f))
            Text(Money.format(o.totalMinor, currency), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 15.sp)
        }
        Text(o.customerPhone, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
        o.address?.let { Text(it, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp, maxLines = 2) }
        Text("${o.itemCount} ${t("delivery.items")}", color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)

        if (!o.isTerminal) {
            Row(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalAlignment = Alignment.CenterVertically) {
                nextStatus(o.status)?.let { next ->
                    SufrixButton(t("delivery.action.$next"), onAdvance, fullWidth = false)
                }
                Spacer(Modifier.weight(1f))
                SufrixButton(t("delivery.add_prep"), onPrep, variant = BtnVariant.GHOST, fullWidth = false)
                SufrixButton(t("delivery.finalize"), onFinalize, variant = BtnVariant.OUTLINE, fullWidth = false)
                SufrixButton(t("delivery.cancel"), onCancel, variant = BtnVariant.DANGER, fullWidth = false)
            }
        }
    }
}

@Composable
private fun FinalizeDialog(model: AppModel, o: DeliveryOrderView, currency: String, onDismiss: () -> Unit) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var method by remember { mutableStateOf(model.paymentMethods.firstOrNull { it.isCash }?.id ?: model.paymentMethods.firstOrNull()?.id) }
    DialogScrim(onDismiss) {
        Text(t("delivery.finalize"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 20.sp)
        Text("${o.customerName} · ${Money.format(o.totalMinor, currency)}", color = c.textSecondary, fontFamily = SufrixFont, fontSize = 13.sp)
        Text(t("delivery.finalize_pay"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp, modifier = Modifier.fillMaxWidth())
        model.paymentMethods.forEach { m ->
            SufrixButton(m.name, { method = m.id }, variant = if (m.id == method) BtnVariant.PRIMARY else BtnVariant.OUTLINE)
        }
        SufrixButton(
            t("delivery.finalize"), {
                method?.let { id -> scope.launch { if (model.finalizeDelivery(o, id)) onDismiss() } }
            },
            loading = model.isBusy, enabled = method != null,
        )
    }
}

@Composable
private fun CancelDialog(model: AppModel, o: DeliveryOrderView, onDismiss: () -> Unit) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var reason by remember { mutableStateOf("") }
    var restock by remember { mutableStateOf(true) }
    DialogScrim(onDismiss) {
        Text(t("delivery.cancel"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 20.sp)
        Text(o.customerName, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 13.sp)
        SufrixTextField(reason, { reason = it }, t("delivery.cancel_reason"))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text(t("delivery.restore_inventory"), color = c.textPrimary, fontFamily = SufrixFont, fontSize = 14.sp, modifier = Modifier.weight(1f))
            Switch(checked = restock, onCheckedChange = { restock = it })
        }
        SufrixButton(
            t("delivery.cancel"), {
                scope.launch { if (model.cancelDelivery(o, reason.ifBlank { null }, restock)) onDismiss() }
            },
            variant = BtnVariant.DANGER, loading = model.isBusy,
        )
    }
}

@Composable
private fun DialogScrim(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    val c = sufrixColors()
    Box(
        Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onDismiss() },
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier.widthIn(max = 460.dp).fillMaxWidth(0.92f).clip(RoundedCornerShape(Radii.lg)).background(c.bg)
                .border(1.dp, c.border, RoundedCornerShape(Radii.lg)).padding(Space.xl)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) { content() }
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
