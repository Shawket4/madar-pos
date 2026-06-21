package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import app.sufrix.core.DiscountView
import app.sufrix.core.PaymentMethodView
import app.sufrix.core.ReceiptView
import app.sufrix.ui.AmountField
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.pressScale
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.launch

// Tender — the checkout overlay. Pick a payment method, take cash (with live
// change), and place the order through the core (online or queued offline). On
// success the same sheet flips to a receipt confirmation. All money + order
// assembly live in the core; this view only collects the tender and renders.
// Mirror of the SwiftUI TenderView.
@Composable
fun TenderOverlay(model: AppModel, currency: String, onClose: () -> Unit) {
    val c = sufrixColors()
    Box(Modifier.fillMaxSize()) {
        // Scrim — tap to dismiss.
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onClose() },
        )
        // Bottom sheet panel (taps don't fall through to the scrim).
        Box(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().fillMaxHeight(0.9f)
                .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
        ) {
            val receipt = model.receipt
            if (receipt != null) {
                ReceiptConfirmation(model, receipt, currency, onDone = onClose)
            } else {
                TenderForm(model, currency, onClose)
            }
        }
    }
}

@Composable
private fun TenderForm(model: AppModel, currency: String, onClose: () -> Unit) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var selected by remember { mutableStateOf<String?>(null) }
    var tendered by remember { mutableStateOf(0L) }
    var tip by remember { mutableStateOf(0L) }
    var customerName by remember { mutableStateOf("") }
    var notes by remember { mutableStateOf("") }

    LaunchedEffect(Unit) {
        if (selected == null) {
            selected = (model.paymentMethods.firstOrNull { it.isCash } ?: model.paymentMethods.firstOrNull())?.id
        }
    }

    val method: PaymentMethodView? = model.paymentMethods.firstOrNull { it.id == selected }
    val isCash = method?.isCash ?: false
    val total = model.cartTotals.totalMinor
    // A tip on a cash order comes out of the same drawer → due with the bill.
    val dueCash = total + (if (isCash) tip else 0L)
    val change = (tendered - dueCash).coerceAtLeast(0L)
    val canPlace = selected != null && !model.isPlacingOrder && (!isCash || tendered >= dueCash)

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.xl),
        verticalArrangement = Arrangement.spacedBy(Space.xl),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(t("order.tender"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 22.sp)
            Box(Modifier.weight(1f))
            Text("✕", color = c.textMuted, fontSize = 18.sp, modifier = Modifier.clickable { onClose() })
        }

        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text(t("order.payment_method"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
            model.paymentMethods.forEach { m ->
                MethodChip(m.name, active = m.id == selected) { selected = m.id }
            }
        }

        val activeDiscounts = model.discounts.filter { it.isActive }
        if (activeDiscounts.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                Text(t("order.discount"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                MethodChip(t("order.no_discount"), active = model.cartDiscountId == null) { model.setDiscount(null) }
                activeDiscounts.forEach { d ->
                    MethodChip(discountLabel(d), active = model.cartDiscountId == d.id) { model.setDiscount(d.id) }
                }
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text(t("order.customer"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
            SufrixTextField(customerName, { customerName = it }, t("order.customer_hint"))
            SufrixTextField(notes, { notes = it }, t("order.notes_hint"))
        }

        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text(t("order.tip"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
            AmountField(amountMinor = tip, onAmountMinor = { tip = it }, currencyCode = currency)
        }

        if (isCash) {
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                Text(t("order.cash_received"), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                AmountField(amountMinor = tendered, onAmountMinor = { tendered = it }, currencyCode = currency)
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            if (model.cartTotals.discountMinor > 0) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(t("order.discount"), color = c.success, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 14.sp)
                    Box(Modifier.weight(1f))
                    Text("−${Money.format(model.cartTotals.discountMinor, currency)}", color = c.success, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                }
            }
            SummaryRow(t("order.total"), Money.format(total, currency), emphasized = true)
            if (tip > 0) SummaryRow(t("order.tip"), Money.format(tip, currency))
            if (isCash) SummaryRow(t("order.change"), Money.format(change, currency))
        }

        model.error?.let { NoticeBanner(it, ChipTone.DANGER) }

        SufrixButton(
            t("order.place_order"),
            {
                val id = selected ?: return@SufrixButton
                scope.launch {
                    model.placeOrder(
                        id, if (isCash) tendered else 0L,
                        tipMinor = tip,
                        customerName = customerName.ifBlank { null },
                        notes = notes.ifBlank { null },
                    )
                }
            },
            loading = model.isPlacingOrder,
            enabled = canPlace,
        )
    }
}

@Composable
private fun MethodChip(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val interaction = remember { MutableInteractionSource() }
    Text(
        label,
        color = if (active) c.textOnAccent else c.textPrimary,
        fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp,
        modifier = Modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
            .background(if (active) c.accent else c.surface)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(vertical = 14.dp, horizontal = Space.lg),
    )
}

@Composable
private fun ReceiptConfirmation(model: AppModel, receipt: ReceiptView, currency: String, onDone: () -> Unit) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        SufrixMark(size = 52.dp)
        Text(t("order.order_placed"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 22.sp)
        StatusChip(
            t(if (receipt.queuedOffline) "order.queued_hint" else "order.sent_hint"),
            if (receipt.queuedOffline) ChipTone.WARNING else ChipTone.SUCCESS,
        )

        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
                .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            receipt.lines.forEach { line ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text("${line.qty}× ${line.name}", color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
                    Box(Modifier.weight(1f))
                    Text(Money.format(line.lineTotalMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                }
            }
        }

        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            SummaryRow(t("order.subtotal"), Money.format(receipt.subtotalMinor, currency))
            if (receipt.discountMinor > 0) {
                SummaryRow(t("order.discount"), "−${Money.format(receipt.discountMinor, currency)}")
            }
            SummaryRow(t("order.tax"), Money.format(receipt.taxMinor, currency))
            SummaryRow(t("order.total"), Money.format(receipt.totalMinor, currency), emphasized = true)
            if (receipt.tipMinor > 0) SummaryRow(t("order.tip"), Money.format(receipt.tipMinor, currency))
            if (receipt.isCash) {
                SummaryRow(t("order.cash_received"), Money.format(receipt.amountTenderedMinor, currency))
                SummaryRow(t("order.change"), Money.format(receipt.changeMinor, currency))
            }
        }

        // Print receipt — best-effort send to the configured network printer.
        when (model.printState) {
            PrintState.PRINTED -> StatusChip(t("receipt.printed"), ChipTone.SUCCESS)
            PrintState.NO_PRINTER -> StatusChip(t("receipt.no_printer"), ChipTone.WARNING)
            else -> SufrixButton(
                if (model.printState == PrintState.FAILED) t("receipt.print_failed") else t("receipt.print"),
                { scope.launch { model.printReceipt() } },
                variant = BtnVariant.OUTLINE,
                loading = model.printState == PrintState.PRINTING,
            )
        }

        SufrixButton(t("order.new_order"), { onDone() })
    }
}

private fun discountLabel(d: DiscountView): String =
    if (d.dtype == "percentage") "${d.name} ${d.value}%" else d.name

@Composable
private fun SummaryRow(label: String, value: String, emphasized: Boolean = false) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium, fontSize = if (emphasized) 16.sp else 14.sp,
        )
        Box(Modifier.weight(1f))
        Text(
            value, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = if (emphasized) FontWeight.Black else FontWeight.SemiBold, fontSize = if (emphasized) 18.sp else 14.sp,
        )
    }
}
