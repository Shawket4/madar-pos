@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.DiscountView
import app.madar.core.CheckoutSplit
import app.madar.core.PaymentMethodView
import app.madar.core.ReceiptView
import app.madar.ui.AmountField
import app.madar.ui.ChipTone
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.BtnVariant
import app.madar.ui.MadarButton
import app.madar.ui.MadarTextField
import app.madar.ui.LocalMadarFont
import app.madar.ui.pressScale
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Elevation
import app.madar.ui.elevation
import kotlinx.coroutines.launch

// Tender — the checkout overlay. Pick a payment method, take cash (with live
// change), and place the order through the core (online or queued offline). On
// success the same sheet flips to a receipt confirmation. All money + order
// assembly live in the core; this view only collects the tender and renders.
// Mirror of the SwiftUI TenderView.
@Composable
fun TenderOverlay(model: AppModel, currency: String, onClose: () -> Unit) {
    val c = madarColors()
    Box(Modifier.fillMaxSize()) {
        // Scrim — tap to dismiss.
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onClose() },
        )
        // Bottom sheet panel (taps don't fall through to the scrim). Capped to the
        // ResponsiveSheet width (≈600) and centered on wide windows, not a full slab.
        Box(
            Modifier.align(Alignment.BottomCenter).widthIn(max = 600.dp).fillMaxWidth().fillMaxHeight(0.9f)
                .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.surfaceRaised)
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
    val c = madarColors()
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
    // A tip paid by cash comes out of the same drawer → due with the bill. The tip
    // can ride a DIFFERENT method than the order (e.g. card order + cash tip), so
    // gate on the TIP method's isCash (tipMethod ?? selected), not the order's.
    val tipMethodIsCash = run {
        if (tip <= 0L) return@run false
        val tm = model.paymentMethods.firstOrNull { it.id == (tipMethod ?: selected) }
        tm?.isCash ?: isCash
    }
    val tipCash = if (tipMethodIsCash) tip else 0L
    val dueCash = total + tipCash
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
        // Sticky header — title + live order total + close. Lives outside the
        // scroll so it pins like the Swift/Flutter sheet header.
        Row(
            Modifier.fillMaxWidth().padding(start = Space.xl, end = Space.xl, top = Space.sm, bottom = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Text(t("order.tender"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 19.sp)
            Box(Modifier.weight(1f))
            Text(Money.format(total, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
            Box(
                Modifier.size(32.dp).clip(CircleShape).background(c.surfaceAlt).clickable { onClose() },
                contentAlignment = Alignment.Center,
            ) {
                MadarIcon("xmark", tint = c.textMuted, size = IconSize.sm)
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))

        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(horizontal = Space.xl, vertical = Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            // Order summary card — totals at a glance. (Flat border, no shadow — matches Swift.)
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
                    .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                SummaryRow(t("order.subtotal"), Money.format(model.cartTotals.subtotalMinor, currency))
                if (model.cartTotals.discountMinor > 0)
                    SummaryRow(t("order.discount"), "−${Money.format(model.cartTotals.discountMinor, currency)}", valueColor = c.success)
                if (model.cartTotals.taxMinor > 0)
                    SummaryRow(t("order.tax"), Money.format(model.cartTotals.taxMinor, currency))
                Box(Modifier.padding(vertical = Space.sm).fillMaxWidth().height(1.dp).background(c.border))
                SummaryRow(t("order.total"), Money.format(total, currency), emphasized = true)
            }

            // Payment — brand-colored method chips, or a split allocator.
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    SectionLabel(t("order.payment_method"))
                    Box(Modifier.weight(1f))
                    if (model.paymentMethods.size > 1) {
                        Row(
                            Modifier.clip(CircleShape).background(if (splitMode) c.accentBg else c.surfaceAlt)
                                .clickable { splitMode = !splitMode }.padding(horizontal = 8.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            MadarIcon(if (splitMode) "checkmark.circle.fill" else "rectangle.split.2x1",
                                tint = if (splitMode) c.accent else c.textMuted, size = IconSize.md)
                            Text(t("order.split_payment"), color = if (splitMode) c.accent else c.textMuted,
                                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
                        }
                    }
                }
                if (splitMode) {
                    model.paymentMethods.forEach { m ->
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                            Box(Modifier.size(9.dp).clip(CircleShape).background(hexColor(m.color)))
                            Text(m.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 96.dp))
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
                        Text(t("order.split_remaining"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 12.sp)
                        Box(Modifier.weight(1f))
                        Text(Money.format(splitRemaining, currency), color = if (splitRemaining == 0L) c.success else c.danger, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 13.sp)
                    }
                } else {
                    // Wrap into an adaptive grid (parity with Swift's FlowLayout) so
                    // many methods flow onto multiple rows instead of overflowing.
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(Space.sm),
                        verticalArrangement = Arrangement.spacedBy(Space.sm),
                    ) {
                        model.paymentMethods.forEach { m -> PayChip(m, active = m.id == selected) { selected = m.id } }
                    }
                }
            }

            // Cash tendered (cash, non-split) — quick chips + change banner.
            if (isCash && !splitMode) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionLabel(t("order.cash_received"))
                    AmountField(amountMinor = tendered, onAmountMinor = { tendered = it }, currencyCode = currency)
                    // Round-number cash presets at or above the amount due (50/100/200/500 major).
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(Space.sm),
                        verticalArrangement = Arrangement.spacedBy(Space.sm),
                    ) {
                        QuickCash(t("order.exact"), tendered == dueCash) { tendered = dueCash }
                        listOf(5000L, 10000L, 20000L, 50000L).filter { it >= dueCash }.take(3).forEach { p ->
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
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    MadarIcon("heart.circle", tint = c.textMuted, size = 13.dp)
                    SectionLabel(t("order.tip"))
                    Box(Modifier.weight(1f))
                    if (tip > 0L) StatusChip(Money.format(tip, currency), ChipTone.SUCCESS, icon = "plus")
                }
                if (model.paymentMethods.size > 1) {
                    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        model.paymentMethods.forEach { m ->
                            val activeTip = (tipMethod ?: selected) == m.id
                            Row(
                                Modifier.clip(CircleShape).background(if (activeTip) hexColor(m.color) else c.surface)
                                    .border(1.dp, if (activeTip) Color.Transparent else c.border, CircleShape)
                                    .clickable { tipMethod = m.id }.padding(horizontal = 11.dp, vertical = 6.dp),
                                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
                            ) {
                                if (activeTip) MadarIcon("checkmark", tint = c.textOnAccent, size = 10.dp)
                                Text(m.name, color = if (activeTip) c.textOnAccent else c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
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
                    // Wrapping pill chips (matches Swift's FlowLayout) — not full-width stacked rows.
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                        DiscountChip(t("order.no_discount"), active = model.cartDiscountId == null) { model.setDiscount(null) }
                        activeDiscounts.forEach { d ->
                            DiscountChip(discountLabel(d), active = model.cartDiscountId == d.id) { model.setDiscount(d.id) }
                        }
                    }
                }
            }

            // Customer + notes.
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                SectionLabel(t("order.customer"))
                MadarTextField(customerName, { customerName = it }, t("order.customer_hint"), icon = "person")
                MadarTextField(notes, { notes = it }, t("order.notes_hint"), icon = "text.bubble")
            }

            model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
        }

        // Sticky footer — Place Order.
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            Box(Modifier.padding(Space.lg)) {
                MadarButton(
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
                    icon = "checkmark",
                )
            }
        }
    }
}

/** Section label — small uppercase muted heading, matching the Swift sectionLabel. */
@Composable
private fun SectionLabel(text: String) {
    Text(text.uppercase(), color = madarColors().textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
}

/** Payment method tile — a per-method icon (in the method's brand color) + label
 *  + check when active; fills with the method's brand color when selected.
 *  Mirrors the Swift PayChip (which maps method.icon to an SF Symbol). */
@Composable
private fun PayChip(method: PaymentMethodView, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    val brand = hexColor(method.color)
    val interaction = remember { MutableInteractionSource() }
    Row(
        // Size to content with a sensible floor so the chips flow onto multiple
        // rows inside the FlowRow instead of one stretching to the full width
        // (which would force one-chip-per-row and overflow with many methods).
        Modifier.widthIn(min = 140.dp).pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
            .background(if (active) brand else c.surface)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(horizontal = 12.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        MadarIcon(payGlyph(method.icon), tint = if (active) c.textOnAccent else brand, size = IconSize.md)
        Text(method.name, color = if (active) c.textOnAccent else c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1)
        Box(Modifier.weight(1f))
        if (active) MadarIcon("checkmark", tint = c.textOnAccent, size = IconSize.sm)
    }
}

/** Map a backend payment-icon token to a glyph (Kotlin glyph convention — no
 *  Material-icons-extended dep). Mirrors the Swift PayChip.symbol() mapping. */
// Mirrors the Flutter PaymentMethodX.uiIcon — backend stores one of these keys.
private fun payGlyph(icon: String): String = when (icon.lowercase()) {
    "money", "cash", "banknote" -> "banknote"
    "credit_card", "card", "creditcard", "visa", "mastercard", "debit" -> "creditcard"
    "wallet", "ewallet", "e_wallet" -> "wallet"
    "pie_chart" -> "chart.pie"
    "delivery" -> "bicycle"
    "qr_code", "qr" -> "qrcode"
    "bank", "transfer", "bank_transfer" -> "bank"
    "gift_card" -> "gift"
    "smartphone", "phone", "mobile", "vodafone", "instapay" -> "iphone"
    "receipt" -> "receipt"
    "store" -> "storefront"
    "star" -> "star"
    "link" -> "link"
    else -> "banknote"
}

/** Discount chip — a content-width pill with a leading check when active, laid out
 *  in a FlowRow. Mirrors the Swift `chip()` (NOT a full-width stacked row). */
@Composable
private fun DiscountChip(label: String, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
            .background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        if (active) MadarIcon("checkmark", tint = c.textOnAccent, size = 10.dp)
        Text(label, color = if (active) c.textOnAccent else c.textPrimary,
            fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

/** A quick-tender amount chip (Exact / round-number presets) that fills cash. */
@Composable
private fun QuickCash(label: String, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    Box(
        Modifier.clip(RoundedCornerShape(Radii.xl)).background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.xl))
            .clickable { onClick() }.padding(horizontal = 14.dp, vertical = 7.dp),
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
    }
}

/** Green "Change due" / red "Short by" banner under the cash field. */
@Composable
private fun ChangeBanner(change: Long, short: Long, currency: String) {
    val c = madarColors()
    val ok = short <= 0L
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(if (ok) c.successBg else c.dangerBg)
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(if (ok) t("order.change_due") else t("order.short_by"), color = if (ok) c.success else c.danger, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Text(Money.format(if (ok) change else short, currency), color = if (ok) c.success else c.danger, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp)
    }
}

@Composable
private fun ReceiptConfirmation(model: AppModel, receipt: ReceiptView, currency: String, onDone: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        // Large status glyph — queued-offline shows a clock, sent shows a check
        // (mirrors the Swift clock.badge.checkmark / checkmark.circle.fill).
        val queued = receipt.queuedOffline
        MadarIcon(
            if (queued) "clock" else "checkmark.circle",
            tint = if (queued) c.warning else c.success,
            size = 44.dp,
        )
        Text(t("order.order_placed"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 22.sp)
        StatusChip(
            t(if (queued) "order.queued_hint" else "order.sent_hint"),
            if (queued) ChipTone.WARNING else ChipTone.SUCCESS,
            icon = if (queued) "clock" else "checkmark.circle",
        )

        // The printable receipt, exactly as it will print (preview before print).
        // Pass the org logo so the on-screen paper shows the brand mark.
        ReceiptPaper(model, receipt, model.branchName, currency, model.orgLogoUrl)

        // The receipt is auto-printed on checkout. Show the print status, and keep
        // a Reprint button available (a reprint does NOT re-pop the cash drawer).
        when (model.printState) {
            PrintState.PRINTED -> StatusChip(t("receipt.printed"), ChipTone.SUCCESS, icon = "checkmark.circle")
            PrintState.NO_PRINTER -> StatusChip(t("receipt.no_printer"), ChipTone.WARNING, icon = "exclamationmark.triangle")
            PrintState.FAILED -> StatusChip(t("receipt.print_failed"), ChipTone.DANGER, icon = "exclamationmark.triangle")
            else -> {}
        }
        MadarButton(
            t("receipt.reprint"),
            { scope.launch { model.printReceipt(kickDrawer = false) } },
            variant = BtnVariant.OUTLINE,
            loading = model.printState == PrintState.PRINTING,
            icon = "printer",
        )

        MadarButton(t("order.new_order"), { onDone() }, variant = BtnVariant.OUTLINE, icon = "plus")
    }
}

private fun discountLabel(d: DiscountView): String =
    if (d.dtype == "percentage") "${d.name} ${d.value}%" else d.name

@Composable
private fun SummaryRow(label: String, value: String, emphasized: Boolean = false, valueColor: Color? = null) {
    val c = madarColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium, fontSize = if (emphasized) 14.sp else 13.sp,
        )
        Box(Modifier.weight(1f))
        Text(
            value, color = valueColor ?: (if (emphasized) c.textPrimary else c.textSecondary), fontFamily = LocalMadarFont.current,
            fontWeight = if (emphasized) FontWeight.Black else FontWeight.SemiBold, fontSize = if (emphasized) 17.sp else 13.sp,
        )
    }
}
