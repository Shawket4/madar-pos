package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
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
import app.sufrix.core.CheckoutSplit
import app.sufrix.core.PaymentMethodView
import app.sufrix.core.ReceiptLineView
import app.sufrix.core.ReceiptModifierView
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
    var tipMethod by remember { mutableStateOf<String?>(null) }
    var customerName by remember { mutableStateOf("") }
    var notes by remember { mutableStateOf("") }
    // Split a single bill across several methods (allocated amounts must sum to total).
    var splitMode by remember { mutableStateOf(false) }
    val splitAmounts = remember { mutableStateMapOf<String, Long>() }

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

    val splitAllocated = splitAmounts.values.sum()
    val splitRemaining = total - splitAllocated
    val splitLegs = splitAmounts.filter { it.value > 0 }.map { CheckoutSplit(it.key, it.value) }
    val splitPrimary = splitAmounts.filter { it.value > 0 }.maxByOrNull { it.value }?.key
    val canPlace = when {
        model.isPlacingOrder -> false
        splitMode -> splitAllocated == total && splitLegs.isNotEmpty()
        else -> selected != null && (!isCash || tendered >= dueCash)
    }

    Column(Modifier.fillMaxSize()) {
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(Space.xl),
            verticalArrangement = Arrangement.spacedBy(Space.xl),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(t("order.tender"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 22.sp)
                Box(Modifier.weight(1f))
                Text("✕", color = c.textMuted, fontSize = 18.sp, modifier = Modifier.clickable { onClose() })
            }

            // Order summary card — totals at a glance.
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surfaceAlt).padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                SummaryRow(t("order.subtotal"), Money.format(model.cartTotals.subtotalMinor, currency))
                if (model.cartTotals.discountMinor > 0)
                    SummaryRow(t("order.discount"), "−${Money.format(model.cartTotals.discountMinor, currency)}")
                if (model.cartTotals.taxMinor > 0)
                    SummaryRow(t("order.tax"), Money.format(model.cartTotals.taxMinor, currency))
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
                SummaryRow(t("order.total"), Money.format(total, currency), emphasized = true)
            }

            // Payment — brand-colored method chips, or a split allocator.
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    SectionLabel(t("order.payment_method"))
                    Box(Modifier.weight(1f))
                    if (model.paymentMethods.size > 1) {
                        Text(
                            (if (splitMode) "● " else "▭ ") + t("order.split_payment"),
                            color = if (splitMode) c.accent else c.textMuted,
                            fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 11.sp,
                            modifier = Modifier.clip(CircleShape).background(if (splitMode) c.accentBg else c.surfaceAlt)
                                .clickable { splitMode = !splitMode }.padding(horizontal = 8.dp, vertical = 4.dp),
                        )
                    }
                }
                if (splitMode) {
                    model.paymentMethods.forEach { m ->
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                            Box(Modifier.size(9.dp).clip(CircleShape).background(hexColor(m.color)))
                            Text(m.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, modifier = Modifier.width(82.dp))
                            Box(Modifier.weight(1f)) {
                                AmountField(amountMinor = splitAmounts[m.id] ?: 0L, onAmountMinor = { splitAmounts[m.id] = it }, currencyCode = currency)
                            }
                        }
                    }
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm))
                            .background(if (splitRemaining == 0L) c.successBg else c.warningBg)
                            .padding(horizontal = Space.md, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(t("order.split_remaining"), color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 12.sp)
                        Box(Modifier.weight(1f))
                        Text(Money.format(splitRemaining, currency), color = if (splitRemaining == 0L) c.success else c.danger, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 13.sp)
                    }
                } else {
                    model.paymentMethods.forEach { m -> PayChip(m, active = m.id == selected) { selected = m.id } }
                }
            }

            // Cash tendered (cash, non-split) — quick chips + change banner.
            if (isCash && !splitMode) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionLabel(t("order.cash_received"))
                    AmountField(amountMinor = tendered, onAmountMinor = { tendered = it }, currencyCode = currency)
                    Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        QuickCash(t("order.exact"), tendered == dueCash) { tendered = dueCash }
                        listOf(5000L, 10000L, 20000L).filter { it >= dueCash }.take(2).forEach { p ->
                            QuickCash(Money.format(p, currency), tendered == p) { tendered = p }
                        }
                    }
                    if (tendered > 0L) ChangeBanner(change, (dueCash - tendered).coerceAtLeast(0L), currency)
                }
            }

            // Tip card — optional, with which method pays the tip.
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surfaceAlt)
                    .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    SectionLabel("♡ " + t("order.tip"))
                    Box(Modifier.weight(1f))
                    if (tip > 0L) StatusChip(Money.format(tip, currency), ChipTone.SUCCESS)
                }
                if (model.paymentMethods.size > 1) {
                    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        model.paymentMethods.forEach { m ->
                            val activeTip = (tipMethod ?: selected) == m.id
                            Row(
                                Modifier.clip(CircleShape).background(if (activeTip) hexColor(m.color) else c.surface)
                                    .border(1.dp, if (activeTip) Color.Transparent else c.border, CircleShape)
                                    .clickable { tipMethod = m.id }.padding(horizontal = 11.dp, vertical = 6.dp),
                                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
                            ) {
                                if (activeTip) Text("✓", color = c.textOnAccent, fontSize = 9.sp, fontWeight = FontWeight.Bold)
                                Text(m.name, color = if (activeTip) c.textOnAccent else c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
                            }
                        }
                    }
                }
                AmountField(amountMinor = tip, onAmountMinor = { tip = it }, currencyCode = currency)
            }

            // Discount.
            val activeDiscounts = model.discounts.filter { it.isActive }
            if (activeDiscounts.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionLabel(t("order.discount"))
                    MethodChip(t("order.no_discount"), active = model.cartDiscountId == null, check = true) { model.setDiscount(null) }
                    activeDiscounts.forEach { d ->
                        MethodChip(discountLabel(d), active = model.cartDiscountId == d.id, check = true) { model.setDiscount(d.id) }
                    }
                }
            }

            // Customer + notes.
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                SectionLabel(t("order.customer"))
                SufrixTextField(customerName, { customerName = it }, t("order.customer_hint"))
                SufrixTextField(notes, { notes = it }, t("order.notes_hint"))
            }

            model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
        }

        // Sticky footer — Place Order.
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            Box(Modifier.padding(Space.lg)) {
                SufrixButton(
                    t("order.place_order"),
                    {
                        scope.launch {
                            if (splitMode) {
                                val primary = splitPrimary ?: return@launch
                                model.placeOrder(primary, 0L, tipMinor = tip, tipPaymentMethodId = tipMethod,
                                    customerName = customerName.ifBlank { null }, notes = notes.ifBlank { null }, splits = splitLegs)
                            } else {
                                val id = selected ?: return@launch
                                model.placeOrder(id, if (isCash) tendered else 0L, tipMinor = tip, tipPaymentMethodId = tipMethod,
                                    customerName = customerName.ifBlank { null }, notes = notes.ifBlank { null })
                            }
                        }
                    },
                    loading = model.isPlacingOrder,
                    enabled = canPlace,
                )
            }
        }
    }
}

/** Section label — small uppercase muted heading, matching the Swift sectionLabel. */
@Composable
private fun SectionLabel(text: String) {
    Text(text.uppercase(), color = sufrixColors().textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
}

/** Payment method tile — a brand-colored dot + label + check when active; fills
 *  with the method's brand color when selected. Mirrors the Swift PayChip. */
@Composable
private fun PayChip(method: PaymentMethodView, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val brand = hexColor(method.color)
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
            .background(if (active) brand else c.surface)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(horizontal = 12.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Box(Modifier.size(10.dp).clip(CircleShape).background(if (active) c.textOnAccent else brand))
        Text(method.name, color = if (active) c.textOnAccent else c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        if (active) Text("✓", color = c.textOnAccent, fontWeight = FontWeight.Bold, fontSize = 13.sp)
    }
}

@Composable
private fun MethodChip(label: String, active: Boolean, check: Boolean = false, onClick: () -> Unit) {
    val c = sufrixColors()
    val interaction = remember { MutableInteractionSource() }
    Text(
        if (active && check) "✓  $label" else label,
        color = if (active) c.textOnAccent else c.textPrimary,
        fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp,
        modifier = Modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
            .background(if (active) c.accent else c.surface)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(vertical = 14.dp, horizontal = Space.lg),
    )
}

/** A quick-tender amount chip (Exact / round-number presets) that fills cash. */
@Composable
private fun QuickCash(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    Box(
        Modifier.clip(RoundedCornerShape(Radii.xl)).background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.xl))
            .clickable { onClick() }.padding(horizontal = 14.dp, vertical = 7.dp),
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 12.sp)
    }
}

/** Green "Change due" / red "Short by" banner under the cash field. */
@Composable
private fun ChangeBanner(change: Long, short: Long, currency: String) {
    val c = sufrixColors()
    val ok = short <= 0L
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(if (ok) c.successBg else c.dangerBg)
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(if (ok) t("order.change_due") else t("order.short_by"), color = if (ok) c.success else c.danger, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Text(Money.format(if (ok) change else short, currency), color = if (ok) c.success else c.danger, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 15.sp)
    }
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

        // The printable receipt, exactly as it will print (preview before print).
        ReceiptPaper(receipt, model.branchName, currency)

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

private fun receiptName(base: String, size: String?): String =
    if (!size.isNullOrEmpty()) "$base ($size)" else base

/** One receipt line with its modifier / bundle breakdown — the on-screen mirror
 *  of the printed item block. */
@Composable
private fun ReceiptLineRow(line: ReceiptLineView, currency: String) {
    val c = sufrixColors()

    @Composable
    fun modifier(prefix: String, m: ReceiptModifierView) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("$prefix${m.name}", color = c.textMuted, fontFamily = SufrixFont, fontSize = 12.sp)
            Box(Modifier.weight(1f))
            if (m.priceMinor > 0) {
                Text("+${Money.format(m.priceMinor, currency)}", color = c.textMuted, fontFamily = SufrixFont, fontSize = 12.sp)
            }
        }
    }

    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("${line.qty}× ${receiptName(line.name, line.sizeLabel)}", color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
            Box(Modifier.weight(1f))
            Text(Money.format(line.lineTotalMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
        }
        if (line.isBundle) {
            line.components.forEach { comp ->
                Text("– ${receiptName(comp.name, comp.sizeLabel)}", color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 12.sp)
                comp.addons.forEach { modifier("   + ", it) }
                comp.optionals.forEach { modifier("   + ", it) }
            }
        } else {
            line.addons.forEach { modifier(" + ", it) }
            line.optionals.forEach { modifier(" + ", it) }
        }
    }
}

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
