@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.runtime.snapshots.SnapshotStateMap
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
import app.madar.ui.Elevation
import app.madar.ui.elevation
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.BtnVariant
import app.madar.ui.MadarButton
import app.madar.ui.MadarSheet
import app.madar.ui.SheetSize
import app.madar.ui.MadarTextField
import app.madar.ui.LocalMadarFont
import app.madar.ui.pressScale
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import kotlinx.coroutines.launch

// Tender — the checkout overlay. Pick a payment method, take cash (with live
// change), and place the order through the core (online or queued offline). On
// success the same sheet flips to a receipt confirmation. All money + order
// assembly live in the core; this view only collects the tender and renders.
// Mirror of the SwiftUI TenderView.
@Composable
fun TenderOverlay(model: AppModel, currency: String, onClose: () -> Unit) {
    // The same branded MadarSheet as the other modals — slides up, dims the scrim,
    // and drags down to dismiss (the old hand-rolled Box had no swipe).
    MadarSheet(onDismiss = onClose, size = SheetSize.LARGE, maxWidth = 600.dp) { dismiss ->
        val receipt = model.receipt
        if (receipt != null) {
            ReceiptConfirmation(model, receipt, currency, onDone = dismiss)
        } else {
            TenderForm(model, currency, dismiss)
        }
    }
}

/** The money breakdown [CheckoutDrawer] renders in its summary card + hero total.
 *  The main checkout feeds it from `model.cartTotals`; the ticket settle feeds it
 *  from the ticket's subtotal (no separate tax/discount projection on a ticket). */
data class CheckoutSummary(
    val subtotalMinor: Long,
    val discountMinor: Long = 0L,
    val taxMinor: Long = 0L,
    val totalMinor: Long,
)

/** The tender the teller collected, handed to [CheckoutDrawer]'s terminal action.
 *  For a normal (non-split) charge `splits` is empty and `primaryMethodId` is the
 *  chosen method; for a split, `splits` carries the legs and `primaryMethodId` is
 *  the largest leg (the method the settle/checkout books against). */
data class CheckoutResult(
    val primaryMethodId: String,
    val tenderedMinor: Long,
    val tipMinor: Long,
    val tipPaymentMethodId: String?,
    val customerName: String?,
    val notes: String?,
    val splits: List<CheckoutSplit>,
    val isCash: Boolean,
)

@Composable
private fun TenderForm(model: AppModel, currency: String, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val totals = model.cartTotals
    CheckoutDrawer(
        model = model,
        currency = currency,
        summary = CheckoutSummary(totals.subtotalMinor, totals.discountMinor, totals.taxMinor, totals.totalMinor),
        title = t("order.tender"),
        terminalLabel = t("order.place_order"),
        terminalIcon = "checkmark",
        placing = model.isPlacingOrder,
        showDiscountPicker = true,
        showCustomerFields = true,
        onClose = onClose,
        onTerminal = { r ->
            scope.launch {
                if (r.splits.isNotEmpty()) {
                    model.placeOrder(r.primaryMethodId, 0L, tipMinor = r.tipMinor, tipPaymentMethodId = r.tipPaymentMethodId,
                        customerName = r.customerName, notes = r.notes, splits = r.splits)
                } else {
                    model.placeOrder(r.primaryMethodId, if (r.isCash) r.tenderedMinor else 0L, tipMinor = r.tipMinor,
                        tipPaymentMethodId = r.tipPaymentMethodId, customerName = r.customerName, notes = r.notes)
                }
            }
        },
    )
}

/** The ONE real checkout drawer — payment method (or split allocator), cash with
 *  live change, tip, optional discount + customer fields, then a terminal button.
 *  Both the main cashier checkout (via [TenderForm]) and the open-ticket settle
 *  (from the Orders channel) drive THIS component; they differ only in the
 *  [summary]/[title]/[terminalLabel] they feed and the [onTerminal] they run — no
 *  mirrored settle UI. Money + order assembly stay in the core / callback; this
 *  view only collects the tender and reports it back via [CheckoutResult].
 *
 *  @param showDiscountPicker cart-only discount chips (a ticket's discount is frozen).
 *  @param showCustomerFields cart-only customer/notes capture (a ticket already
 *    carries its covering + notes).
 *  @param headerContent optional extra header rows under the title (e.g. a ticket
 *    ref chip / the settle line-item review) so the drawer stays self-contained. */
@Composable
fun CheckoutDrawer(
    model: AppModel,
    currency: String,
    summary: CheckoutSummary,
    title: String,
    terminalLabel: String,
    terminalIcon: String,
    placing: Boolean,
    onClose: () -> Unit,
    onTerminal: (CheckoutResult) -> Unit,
    showDiscountPicker: Boolean = false,
    showCustomerFields: Boolean = false,
    headerContent: (@Composable () -> Unit)? = null,
) {
    val c = madarColors()
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
    val total = summary.totalMinor
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
    val short = (dueCash - tendered).coerceAtLeast(0L)

    val splitAllocated = splitAmounts.values.sum()
    val splitRemaining = total - splitAllocated
    val splitLegs = splitAmounts.filter { it.value > 0 }.map { CheckoutSplit(it.key, it.value) }
    val splitPrimary = splitAmounts.filter { it.value > 0 }.maxByOrNull { it.value }?.key
    val canPlace = when {
        placing -> false
        splitMode -> splitAllocated == total && splitLegs.isNotEmpty()
        else -> selected != null && (!isCash || tendered >= dueCash)
    }

    Column(Modifier.fillMaxSize()) {
        // Sticky header — title + live order total + close. Lives outside the
        // scroll so it pins like the Swift/Flutter sheet header.
        TenderHeader(total, currency, onClose, title, Modifier.fillMaxWidth())
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))

        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(horizontal = Space.xl, vertical = Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            // Caller-supplied header block (e.g. the settle line-item review) — sits
            // above the summary so the teller sees WHAT they're charging first.
            headerContent?.invoke()

            // Order summary card — subtotal/discount/tax light above, the grand
            // total in a tinted teal block (the figure tellers look at). Matches
            // the Order screen's CartFooter total block.
            SummaryCard(summary, currency, Modifier.fillMaxWidth())

            // Payment — brand-colored method chips, or a split allocator.
            PaymentSection(
                model, currency, splitMode, onToggleSplit = { splitMode = !splitMode },
                selected = selected, onSelect = { selected = it },
                splitAmounts = splitAmounts, splitRemaining = splitRemaining,
                modifier = Modifier.fillMaxWidth(),
            )

            // Cash tendered (cash, non-split) — hero amount-due block, quick chips,
            // and a live change banner.
            if (isCash && !splitMode) {
                CashSection(
                    dueCash, tendered, change, short, currency,
                    onTendered = { tendered = it }, modifier = Modifier.fillMaxWidth(),
                )
            }

            // Tip card — optional, with which method pays the tip.
            TipCard(
                model, tip, currency, selected = selected, tipMethod = tipMethod,
                onTip = { tip = it }, onTipMethod = { tipMethod = it },
                modifier = Modifier.fillMaxWidth(),
            )

            // Discount (cart only — a ticket's discount is frozen at fire time).
            if (showDiscountPicker) DiscountSection(model, Modifier.fillMaxWidth())

            // Customer + notes (cart only — a ticket already carries its covering).
            if (showCustomerFields) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionLabel(t("order.customer"))
                    MadarTextField(customerName, { customerName = it }, t("order.customer_hint"), icon = "person")
                    MadarTextField(notes, { notes = it }, t("order.notes_hint"), icon = "text.bubble")
                }
            }

            model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
        }

        // Sticky footer — the terminal action (Place Order / Settle).
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            Box(Modifier.padding(Space.lg)) {
                MadarButton(
                    terminalLabel,
                    {
                        val primary = if (splitMode) splitPrimary else selected
                        primary ?: return@MadarButton
                        onTerminal(
                            CheckoutResult(
                                primaryMethodId = primary,
                                tenderedMinor = tendered,
                                tipMinor = tip,
                                tipPaymentMethodId = tipMethod,
                                customerName = customerName.ifBlank { null },
                                notes = notes.ifBlank { null },
                                splits = if (splitMode) splitLegs else emptyList(),
                                isCash = isCash,
                            ),
                        )
                    },
                    loading = placing,
                    enabled = canPlace,
                    icon = terminalIcon,
                )
            }
        }
    }
}

/** Sticky sheet header — bold title + the live order total in hero teal + a close
 *  affordance. Mirrors the Swift tenderForm header. */
@Composable
private fun TenderHeader(total: Long, currency: String, onClose: () -> Unit, title: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    Row(
        modifier.padding(start = Space.xl, end = Space.xl, top = Space.sm, bottom = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text(title, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 19.sp)
        Box(Modifier.weight(1f))
        Text(Money.format(total, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
        Box(
            Modifier.size(32.dp).clip(CircleShape).background(c.surfaceAlt).clickable { onClose() },
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon("xmark", tint = c.textMuted, size = IconSize.sm)
        }
    }
}

/** Order totals card — subtotal/discount/tax in light muted rows, then the grand
 *  total in a tinted teal block (bold teal figure). Matches the Order screen's
 *  total block: the sub-rows stay light so the total carries the weight. */
@Composable
private fun SummaryCard(summary: CheckoutSummary, currency: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    Column(
        modifier.elevation(Elevation.CARD, RoundedCornerShape(Radii.md)).clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        SummaryRow(t("order.subtotal"), Money.format(summary.subtotalMinor, currency))
        if (summary.discountMinor > 0)
            SummaryRow(t("order.discount"), "−${Money.format(summary.discountMinor, currency)}", valueColor = c.success)
        if (summary.taxMinor > 0)
            SummaryRow(t("order.tax"), Money.format(summary.taxMinor, currency))
        // Grand-total block — tinted teal, the hero figure (matches CartFooter).
        Row(
            Modifier.fillMaxWidth().padding(top = Space.xs).clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
                .padding(horizontal = Space.md, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t("order.total"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
            Box(Modifier.weight(1f))
            Text(Money.format(summary.totalMinor, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
        }
    }
}

/** Payment — the section label + split toggle, then either a brand-color method
 *  grid or the split allocator. */
@Composable
private fun PaymentSection(
    model: AppModel,
    currency: String,
    splitMode: Boolean,
    onToggleSplit: () -> Unit,
    selected: String?,
    onSelect: (String) -> Unit,
    splitAmounts: SnapshotStateMap<String, Long>,
    splitRemaining: Long,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            SectionLabel(t("order.payment_method"))
            Box(Modifier.weight(1f))
            if (model.paymentMethods.size > 1) {
                Row(
                    Modifier.clip(CircleShape).background(if (splitMode) c.accentBg else c.surfaceAlt)
                        .clickable { onToggleSplit() }.padding(horizontal = 8.dp, vertical = 4.dp),
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
            SplitAllocator(model, currency, splitAmounts, splitRemaining, Modifier.fillMaxWidth())
        } else {
            MethodGrid(model, selected, onSelect)
        }
    }
}

/** Two-column grid of payment-method chips — the SHARED method selector used by
 *  both the checkout and the settle sheet (parity with Swift's adaptive LazyVGrid). */
@Composable
internal fun MethodGrid(model: AppModel, selected: String?, onSelect: (String) -> Unit, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        model.paymentMethods.chunked(2).forEach { pair ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                pair.forEach { m -> PayChip(m, active = m.id == selected, onClick = { onSelect(m.id) }, modifier = Modifier.weight(1f)) }
                if (pair.size == 1) Box(Modifier.weight(1f))
            }
        }
    }
}

/** Per-method amount entry + a live remaining indicator (must reach 0). */
@Composable
private fun SplitAllocator(
    model: AppModel,
    currency: String,
    splitAmounts: SnapshotStateMap<String, Long>,
    splitRemaining: Long,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
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
    }
}

/** Cash tendered — a tinted teal "Amount Due" hero block, the cash field, round
 *  presets, and a live change banner. */
@Composable
internal fun CashSection(
    dueCash: Long,
    tendered: Long,
    change: Long,
    short: Long,
    currency: String,
    onTendered: (Long) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        SectionLabel(t("order.cash_received"))
        // Amount-due hero block — tinted teal, the figure the cash tendered must
        // reach (mirrors the grand-total block in weight + treatment).
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
                .padding(horizontal = Space.md, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t("order.total"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 13.sp)
            Box(Modifier.weight(1f))
            Text(Money.format(dueCash, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 18.sp)
        }
        AmountField(amountMinor = tendered, onAmountMinor = onTendered, currencyCode = currency)
        // Round-number cash presets at or above the amount due (50/100/200/500 major).
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            QuickCash(t("order.exact"), tendered == dueCash, onClick = { onTendered(dueCash) })
            listOf(5000L, 10000L, 20000L, 50000L).filter { it >= dueCash }.take(3).forEach { p ->
                QuickCash(Money.format(p, currency), tendered == p, onClick = { onTendered(p) })
            }
        }
        if (tendered > 0L) ChangeBanner(change, short, currency, Modifier.fillMaxWidth())
    }
}

/** Tip card — optional, with which method pays the tip. */
@Composable
private fun TipCard(
    model: AppModel,
    tip: Long,
    currency: String,
    selected: String?,
    tipMethod: String?,
    onTip: (Long) -> Unit,
    onTipMethod: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    Column(
        modifier.elevation(Elevation.CARD, RoundedCornerShape(Radii.md)).clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
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
                        Modifier.clip(CircleShape).background(if (activeTip) hexColor(m.color) else c.surfaceAlt)
                            .border(1.dp, if (activeTip) Color.Transparent else c.border, CircleShape)
                            .clickable { onTipMethod(m.id) }.padding(horizontal = 11.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        if (activeTip) MadarIcon("checkmark", tint = c.textOnAccent, size = 10.dp)
                        Text(m.name, color = if (activeTip) c.textOnAccent else c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
                    }
                }
            }
        }
        AmountField(amountMinor = tip, onAmountMinor = onTip, currencyCode = currency)
    }
}

/** Discount — wrapping pill chips (No discount + each active discount). */
@Composable
private fun DiscountSection(model: AppModel, modifier: Modifier = Modifier) {
    val activeDiscounts = model.discounts.filter { it.isActive }
    if (activeDiscounts.isNotEmpty()) {
        Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            SectionLabel(t("order.discount"))
            // Wrapping pill chips (matches Swift's FlowLayout) — not full-width stacked rows.
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                DiscountChip(t("order.no_discount"), active = model.cartDiscountId == null, onClick = { model.setDiscount(null) })
                activeDiscounts.forEach { d ->
                    DiscountChip(discountLabel(d), active = model.cartDiscountId == d.id, onClick = { model.setDiscount(d.id) })
                }
            }
        }
    }
}

/** Section label — small uppercase muted heading, matching the Swift sectionLabel. */
@Composable
private fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(text.uppercase(), modifier = modifier, color = madarColors().textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
}

/** Payment method tile — a per-method icon (in the method's brand color) + label
 *  + check when active; fills with the method's brand color when selected.
 *  Mirrors the Swift PayChip (which maps method.icon to an SF Symbol). */
@Composable
private fun PayChip(method: PaymentMethodView, active: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = madarColors()
    val brand = hexColor(method.color)
    val interaction = remember { MutableInteractionSource() }
    Row(
        // Fills its grid cell (MethodGrid lays the chips out two-up).
        modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
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
private fun DiscountChip(label: String, active: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Row(
        modifier.pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
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
private fun QuickCash(label: String, active: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Box(
        modifier.pressScale(interaction).clip(RoundedCornerShape(Radii.pill)).background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.pill))
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(horizontal = 14.dp, vertical = 7.dp),
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
    }
}

/** Green "Change due" / red "Short by" banner under the cash field — a leading
 *  tone icon + the hero change figure. */
@Composable
private fun ChangeBanner(change: Long, short: Long, currency: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    val ok = short <= 0L
    val fg = if (ok) c.success else c.danger
    Row(
        modifier.clip(RoundedCornerShape(Radii.sm)).background(if (ok) c.successBg else c.dangerBg)
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        MadarIcon(if (ok) "checkmark.circle.fill" else "exclamationmark.triangle.fill", tint = fg, size = IconSize.lg)
        Text(if (ok) t("order.change_due") else t("order.short_by"), color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Text(Money.format(if (ok) change else short, currency), color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp)
    }
}

@Composable
private fun ReceiptConfirmation(model: AppModel, receipt: ReceiptView, currency: String, onDone: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val queued = receipt.queuedOffline
    // Fixed status header · scrolling receipt · pinned footer — so the print
    // controls + New Order stay reachable however long the receipt is (was one
    // big scroll that pushed the buttons off-screen on a long order). Mirrors Swift.
    Column(Modifier.fillMaxSize()) {
        // ── Fixed status header ──
        Column(
            Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
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
        }

        // ── Scrolling receipt (only the paper scrolls) ──
        Column(
            Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = Space.xl),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            ReceiptPaper(model, receipt, model.branchName, currency, model.orgLogoUrl)
        }

        // ── Pinned footer (surface + top hairline): print status + actions ──
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                when (model.printState) {
                    PrintState.PRINTED -> StatusChip(t("receipt.printed"), ChipTone.SUCCESS, icon = "checkmark.circle")
                    PrintState.NO_PRINTER -> StatusChip(t("receipt.no_printer"), ChipTone.WARNING, icon = "exclamationmark.triangle")
                    PrintState.FAILED -> StatusChip(t("receipt.print_failed"), ChipTone.DANGER, icon = "exclamationmark.triangle")
                    else -> {}
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    MadarButton(
                        t("receipt.reprint"),
                        { scope.launch { model.printReceipt(kickDrawer = false) } },
                        modifier = Modifier.weight(1f),
                        variant = BtnVariant.OUTLINE,
                        loading = model.printState == PrintState.PRINTING,
                        icon = "printer",
                    )
                    MadarButton(t("order.new_order"), { onDone() }, modifier = Modifier.weight(1f), icon = "plus")
                }
            }
        }
    }
}

private fun discountLabel(d: DiscountView): String =
    if (d.dtype == "percentage") "${d.name} ${d.value}%" else d.name

@Composable
private fun SummaryRow(label: String, value: String, valueColor: Color? = null, modifier: Modifier = Modifier) {
    val c = madarColors()
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label, color = c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = FontWeight.Medium, fontSize = 13.sp,
        )
        Box(Modifier.weight(1f))
        Text(
            value, color = valueColor ?: c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
        )
    }
}
