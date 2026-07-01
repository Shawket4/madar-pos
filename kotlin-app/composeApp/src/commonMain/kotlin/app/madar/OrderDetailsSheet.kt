@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.DeliveryOrderView
import app.madar.core.TicketLineView
import app.madar.core.TicketView
import app.madar.ui.ChipTone
import app.madar.ui.Elevation
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarIcon
import app.madar.ui.Money
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.elevation
import app.madar.ui.madarColors
import app.madar.ui.t

// Shared "what's actually in this order" surface for the Orders channel. Both the
// Open-tickets tab (real TicketView.lines) and the Delivery tab (the delivery
// order's money breakdown + context) route through the SAME layout: a context
// header, a line-items / breakdown card, and a totals block. Read-only — the
// settle / lifecycle actions live on the cards + the CheckoutDrawer.

/** Ticket details — the covering + table/guests context, the real line items
 *  (qty × name, size, modifiers, per-line price), and the total. Rendered as the
 *  body of a HUG/LARGE MadarSheet. */
@Composable
fun ColumnScope.TicketDetailsSheet(ticket: TicketView, currency: String) {
    val c = madarColors()
    Column(
        // weight(1f) so a pinned CTA (e.g. Settle) placed after this in the sheet's
        // ColumnScope stays on screen while only the details scroll.
        Modifier.weight(1f, fill = false).fillMaxWidth().verticalScroll(rememberScrollState())
            .padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        // Title row — ticket ref + live status chip.
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            MadarIcon("doc.text", tint = c.accent, size = IconSize.lg)
            Text(ticket.ticketRef ?: t("waiter.ticket"), style = Type.h2(), color = c.textPrimary)
            StatusChip(t("ticket.status.${ticket.status}"), ticketDetailTone(ticket.status))
            if (ticket.queuedOffline) StatusChip(t("waiter.queued"), ChipTone.WARNING, icon = "tray.and.arrow.up")
        }

        // Context chips — customer / table / covers, only when present.
        val ctx = buildList {
            // Who took the table — the waiter who opened the ticket.
            ticket.waiterName?.takeIf { it.isNotBlank() }?.let { add("fork.knife" to "${t("order.waiter")}: $it") }
            ticket.customerName?.takeIf { it.isNotBlank() }?.let { add("person.fill" to it) }
            ticket.tableId?.takeIf { it.isNotBlank() }?.let { add("square.grid.2x2" to "${t("order.table")} $it") }
            ticket.guestCount?.takeIf { it > 0 }?.let { add("person.2.fill" to "$it ${t("waiter.covers")}") }
        }
        if (ctx.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                ctx.forEach { (icon, label) -> ContextChip(icon, label) }
            }
        }

        // Line items card — the real ticket lines. Voided lines strike through.
        OrderLinesCard(ticket.lines, currency)

        // Totals block — a ticket carries a single frozen subtotal (== total).
        TotalsBlock(
            rows = emptyList(),
            total = ticket.subtotalMinor,
            currency = currency,
        )
    }
}

/** Delivery details — the customer/address/channel context and the money
 *  breakdown (subtotal, discount, delivery fee, total). The delivery projection
 *  carries no per-line items (only `itemCount`), so the item count is surfaced as
 *  a summary row rather than a line list. */
@Composable
fun ColumnScope.DeliveryDetailsSheet(o: DeliveryOrderView, currency: String) {
    val c = madarColors()
    Column(
        // weight(1f) so a pinned CTA (e.g. Finalize) placed after this in the sheet's
        // ColumnScope stays on screen while only the details scroll.
        Modifier.weight(1f, fill = false).fillMaxWidth().verticalScroll(rememberScrollState())
            .padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        // Title row — order ref + status + channel.
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            MadarIcon("bicycle", tint = c.accent, size = IconSize.lg)
            Text(o.orderRef ?: t("delivery.title"), style = Type.h2(), color = c.textPrimary)
            StatusChip(t("delivery.status.${o.status}"), deliveryDetailTone(o.status))
            StatusChip(t("delivery.${o.channel}"), ChipTone.NEUTRAL)
        }

        // Context — customer name/phone, address, delivery notes.
        Column(
            Modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
                .clip(RoundedCornerShape(Radii.md)).background(c.surface)
                .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            DetailRow("person.fill", t("receipt.customer"), o.customerName)
            if (o.customerPhone.isNotBlank()) DetailRow("phone.fill", t("receipt.phone"), o.customerPhone)
            o.address?.takeIf { it.isNotBlank() }?.let { DetailRow("mappin.and.ellipse", t("receipt.address").removeSuffix(":"), it) }
            o.paymentHint?.takeIf { it.isNotBlank() }?.let { DetailRow("creditcard", t("order.payment_method"), it) }
        }

        // Customer delivery instructions — warning-tinted so it can't be missed.
        o.deliveryNotes?.takeIf { it.isNotBlank() }?.let { note ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.warningBg)
                    .padding(horizontal = Space.md, vertical = Space.sm),
                verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                MadarIcon("text.bubble", tint = c.warning, size = IconSize.sm)
                Text(note, color = c.warning, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 13.sp)
            }
        }

        // Line items — the real priced lines from the frozen cart snapshot, shown
        // through the SAME card tickets use.
        OrderLinesCard(o.lines, currency)

        // Money breakdown — the delivery projection gives a full total breakdown.
        TotalsBlock(
            rows = buildList {
                add(t("order.subtotal") to Money.format(o.subtotalMinor, currency))
                if (o.discountMinor > 0) add(t("order.discount") to "−${Money.format(o.discountMinor, currency)}")
                if (o.deliveryFeeMinor > 0) add(t("receipt.delivery_fee") to Money.format(o.deliveryFeeMinor, currency))
            },
            total = o.totalMinor,
            currency = currency,
        )
    }
}

/** Line-items card — one row per [TicketLineView]: `qty× name`, size + modifiers
 *  under it, and the per-line price on the trailing edge. Voided lines strike. */
@Composable
private fun OrderLinesCard(lines: List<TicketLineView>, currency: String) {
    val c = madarColors()
    Column(
        Modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
            .clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Text(t("order.items").uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
        if (lines.isEmpty()) {
            Text(t("order.cart_empty"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
        } else {
            lines.forEach { line -> OrderLineRow(line, currency) }
        }
    }
}

/** A single order line — qty badge + name (+ size / modifiers) + per-line total. */
@Composable
private fun OrderLineRow(line: TicketLineView, currency: String) {
    val c = madarColors()
    val strike = if (line.voided) TextDecoration.LineThrough else null
    val nameColor = if (line.voided) c.textMuted else c.textPrimary
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
        // Qty badge — teal pill so the count reads at a glance.
        Box(
            Modifier.clip(RoundedCornerShape(Radii.xs)).background(c.accentBg)
                .padding(horizontal = Space.sm, vertical = 3.dp),
        ) {
            Text("${line.qty}×", color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(line.name, color = nameColor, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, textDecoration = strike)
            // Size + modifiers as a light secondary line (add-ons / options).
            val detail = buildList {
                line.sizeLabel?.takeIf { it.isNotBlank() }?.let { add(it) }
                addAll(line.modifiers)
            }
            if (detail.isNotEmpty()) {
                Text(detail.joinToString(" · "), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 12.sp, textDecoration = strike)
            }
        }
        Text(Money.format(line.lineTotalMinor, currency), style = Type.money(14.sp, FontWeight.Bold), color = nameColor, modifier = Modifier.padding(top = 2.dp))
    }
}

/** Totals block — light muted breakdown rows above a tinted-teal grand-total (the
 *  hero figure), matching the CheckoutDrawer / CartFooter total block. */
@Composable
private fun TotalsBlock(rows: List<Pair<String, String>>, total: Long, currency: String) {
    val c = madarColors()
    Column(
        Modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
            .clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        rows.forEach { (label, value) ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(label, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 13.sp)
                Box(Modifier.weight(1f))
                if (value.isNotBlank()) Text(value, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            }
        }
        Row(
            Modifier.fillMaxWidth().padding(top = Space.xs).clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
                .padding(horizontal = Space.md, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t("order.total"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
            Box(Modifier.weight(1f))
            Text(Money.format(total, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
        }
    }
}

/** A labelled context row — leading icon tile + label + value. */
@Composable
private fun DetailRow(icon: String, label: String, value: String) {
    val c = madarColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
        MadarIcon(icon, tint = c.textMuted, size = IconSize.md)
        Text(label, color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
        Box(Modifier.weight(1f))
        Text(value, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

/** A pill of context (customer / table / covers) with a leading icon. */
@Composable
private fun ContextChip(icon: String, label: String) {
    val c = madarColors()
    Row(
        Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.surfaceAlt)
            .padding(horizontal = Space.md, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        MadarIcon(icon, tint = c.textSecondary, size = IconSize.sm)
        Text(label, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
    }
}

private fun ticketDetailTone(status: String): ChipTone = when (status) {
    "ready" -> ChipTone.SUCCESS
    "queued" -> ChipTone.WARNING
    "settled" -> ChipTone.NEUTRAL
    else -> ChipTone.ACCENT
}

private fun deliveryDetailTone(status: String): ChipTone = when (status) {
    "ready", "delivered" -> ChipTone.SUCCESS
    "preparing" -> ChipTone.WARNING
    "cancelled", "rejected" -> ChipTone.DANGER
    else -> ChipTone.ACCENT
}
