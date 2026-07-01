
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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.TicketView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.EmptyState
import app.madar.ui.MadarButton
import app.madar.ui.MadarIcon
import app.madar.ui.MadarSheet
import app.madar.ui.MadarTextField
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.ScreenHeader
import app.madar.ui.SheetSize
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.elevation
import app.madar.ui.Elevation
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarColors
import app.madar.ui.Space
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.screenHeaderBar
import app.madar.ui.t
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import kotlinx.coroutines.launch

// Waiter open-tickets list — a sub-screen over the SHARED order screen. The waiter
// reuses the teller's OrderScreen (full menu/cart + app chrome), FIRING a round
// instead of tendering; this screen lists the branch's open/ready tickets. "Add
// round" returns to the order screen targeting that ticket; "void" cancels it.
@Composable
fun WaiterTicketsListScreen(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    var voiding by remember { mutableStateOf<TicketView?>(null) }

    LaunchedEffect(Unit) { model.loadOpenTickets() }
    LaunchedEffect(model.ticketTick) { model.loadOpenTickets() }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxSize()) {
            Box(Modifier.screenHeaderBar()) {
                ScreenHeader(
                    t("waiter.tickets"),
                    onBack = { model.showTickets = false },
                ) {
                    MadarIcon(
                        "arrow.clockwise", tint = c.textSecondary, size = IconSize.md,
                        modifier = Modifier.clickable { scope.launch { model.loadOpenTickets() } },
                    )
                }
            }
            model.error?.let {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner(it, tone = ChipTone.WARNING, icon = "exclamationmark.circle")
                }
            }

            if (model.openTickets.isEmpty()) {
                EmptyState("tray", t("waiter.no_tickets"))
            } else {
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(Space.lg),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    items(model.openTickets, key = { it.id }) { ticket ->
                        TicketRow(
                            ticket, currency,
                            // "Add round": target this ticket, return to the order screen
                            // (its Fire button becomes "Add round" and fires into it).
                            onAddRound = { model.clearCart(); model.activeTicketId = ticket.id; model.showTickets = false },
                            onVoid = { voiding = ticket },
                        )
                    }
                }
            }
        }
        voiding?.let { ticket ->
            MadarSheet(onDismiss = { voiding = null }, size = SheetSize.HUG, maxWidth = 480.dp) { dismiss ->
                VoidSheetContent(ticket, dismiss, onConfirm = { reason ->
                    scope.launch { model.voidTicket(ticket.id, reason); dismiss() }
                })
            }
        }
    }
}

// ── POS-side settle surface (cashier) — the "Open tickets" tab of the unified
// Orders surface. No nav header of its own (the unified screen owns back + title
// + the tab bar). Live: reloads on `ticketTick` so a waiter's fire/round/settle/
// void from another device appears here instantly (no manual refresh). ──────────
@Composable
fun TicketsSettleBody(model: AppModel) {
    val c = madarColors()
    val currency = model.session?.currencyCode ?: ""
    // Two overlays: viewing a ticket's order details, and settling it. Both are
    // nested here (inside IncomingScreen's overlay layer). Settling reuses the ONE
    // shared CheckoutDrawer — no mirrored settle UI.
    var viewing by remember { mutableStateOf<TicketView?>(null) }
    var settling by remember { mutableStateOf<TicketView?>(null) }
    val settleable = model.openTickets.filter { it.status == "open" || it.status == "ready" }

    LaunchedEffect(Unit) { model.loadOpenTickets() }
    LaunchedEffect(model.ticketTick) { model.loadOpenTickets() }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxSize()) {
            model.error?.let {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner(it, tone = ChipTone.WARNING, icon = "exclamationmark.circle")
                }
            }
            if (settleable.isEmpty()) {
                EmptyState("tray", t("waiter.no_tickets"))
            } else {
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(Space.lg),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    items(settleable, key = { it.id }) { ticket ->
                        Box(Modifier.widthIn(max = 620.dp).fillMaxWidth()) {
                            SettleTicketCard(
                                ticket, currency,
                                onView = { viewing = ticket },
                                onSettle = { settling = ticket },
                            )
                        }
                    }
                }
            }
        }

        // ── Order-details overlay — the SHARED details layout (real line items). ──
        viewing?.let { ticket ->
            MadarSheet(onDismiss = { viewing = null }, size = SheetSize.LARGE, maxWidth = 560.dp) { dismiss ->
                TicketDetailsSheet(ticket, currency)
                // Settle CTA pinned under the details.
                Box(Modifier.fillMaxWidth().padding(Space.lg)) {
                    MadarButton(t("waiter.settle"), { dismiss(); settling = ticket }, icon = "checkmark.circle")
                }
            }
        }

        // ── Settle overlay — the ONE real CheckoutDrawer (same as the cashier
        // checkout), driven by the ticket total; terminal action = settle. Stays on
        // the list after settling (the ticket drops out on reload). ──
        settling?.let { ticket ->
            MadarSheet(onDismiss = { settling = null }, size = SheetSize.LARGE, maxWidth = 600.dp) { dismiss ->
                TicketSettleDrawer(model, ticket, currency, dismiss)
            }
        }
    }
}

/** Settle a ticket through the SHARED [CheckoutDrawer] — same payment/cash/tip
 *  flow as the cashier checkout. The ticket's line-item review rides in as the
 *  drawer's header content, the ticket subtotal drives the total, and the terminal
 *  action settles the ticket into a paid order via [AppModel.settleTicket]. */
@Composable
private fun TicketSettleDrawer(model: AppModel, ticket: TicketView, currency: String, dismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    CheckoutDrawer(
        model = model,
        currency = currency,
        summary = CheckoutSummary(subtotalMinor = ticket.subtotalMinor, totalMinor = ticket.subtotalMinor),
        title = t("waiter.settle"),
        terminalLabel = t("waiter.settle"),
        terminalIcon = "checkmark.circle",
        placing = model.isBusy,
        onClose = dismiss,
        headerContent = { TicketSettleHeader(ticket, currency) },
        onTerminal = { r ->
            scope.launch {
                val ok = model.settleTicket(
                    ticket.id, r.primaryMethodId,
                    amountTenderedMinor = if (r.isCash && r.tenderedMinor > 0) r.tenderedMinor else null,
                    tipMinor = if (r.tipMinor > 0) r.tipMinor else null,
                    tipPaymentMethodId = if (r.tipMinor > 0) (r.tipPaymentMethodId ?: r.primaryMethodId) else null,
                )
                if (ok) dismiss()
            }
        },
    )
}

/** Compact line-item review shown at the top of the settle drawer — the cashier
 *  sees WHAT they're charging (a strike on voided lines) before tendering. */
@Composable
private fun TicketSettleHeader(ticket: TicketView, currency: String) {
    val c = madarColors()
    Column(
        Modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
            .clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            Text(ticket.ticketRef ?: t("waiter.ticket"), style = Type.title(), color = c.textPrimary)
            TicketStatusChip(ticket.status)
        }
        ticket.lines.forEach { line ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                Text(
                    "${line.qty}× ${line.name}",
                    style = Type.bodySm(), color = c.textSecondary,
                    textDecoration = if (line.voided) TextDecoration.LineThrough else null,
                    modifier = Modifier.weight(1f),
                )
                Text(Money.format(line.lineTotalMinor, currency), style = Type.money(13.sp, FontWeight.SemiBold), color = c.textPrimary)
            }
        }
    }
}

/** Status pill for a ticket — maps the ticket state to a shared [StatusChip] tone
 *  (ready → success, queued → warning, settled → neutral, else accent). */
@Composable
private fun TicketStatusChip(status: String) {
    StatusChip(t("ticket.status.$status"), ticketStatusTone(status))
}

private fun ticketStatusTone(status: String): ChipTone = when (status) {
    "ready" -> ChipTone.SUCCESS
    "queued" -> ChipTone.WARNING
    "settled" -> ChipTone.NEUTRAL
    else -> ChipTone.ACCENT
}

// Ticket status → (foreground, tinted-background) pair for the card's header strip.
// Mirrors the Delivery/Kitchen tint pattern so the ticket state reads at a glance.
private fun ticketStatusTint(status: String, c: MadarColors): Pair<Color, Color> = when (status) {
    "ready" -> c.success to c.successBg
    "queued" -> c.warning to c.warningBg
    "settled" -> c.textSecondary to c.surfaceAlt
    else -> c.accent to c.accentBg
}

/** Open-ticket card on the waiter board — a status-tinted header strip (ref + state
 *  + bold-teal total) over a body with the covering customer and inline "Add round"
 *  / "Void" actions. Mirrors the Delivery open-order card so the two boards match. */
@Composable
private fun TicketRow(
    ticket: TicketView,
    currency: String,
    onAddRound: () -> Unit,
    onVoid: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    val (statusFg, statusBg) = ticketStatusTint(ticket.status, c)
    val live = ticket.status == "open" || ticket.status == "ready"
    val shape = RoundedCornerShape(Radii.md)
    Column(
        modifier.fillMaxWidth()
            .elevation(Elevation.CARD, shape)
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.borderLight, shape),
    ) {
        // Status-tinted header strip — fixed height so every card's body starts at the
        // same y; status dot + bold ref + state lead, money is the hero on the trailing
        // edge in a tinted teal block.
        Row(
            Modifier.fillMaxWidth().height(56.dp).background(statusBg).padding(horizontal = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(statusFg))
            Text(
                ticket.ticketRef ?: t("waiter.ticket"),
                color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Black, fontSize = 19.sp, maxLines = 1,
            )
            TicketStatusChip(ticket.status)
            if (ticket.queuedOffline) StatusChip(t("waiter.queued"), ChipTone.WARNING, icon = "tray.and.arrow.up")
            Spacer(Modifier.weight(1f))
            Box(
                Modifier.clip(RoundedCornerShape(Radii.sm)).background(c.accentBg)
                    .padding(horizontal = Space.md, vertical = 7.dp),
            ) {
                Text(Money.format(ticket.subtotalMinor, currency), style = Type.money(16.sp, FontWeight.Black), color = c.accent)
            }
        }
        val name = ticket.customerName?.takeIf { it.isNotBlank() }
        if (name != null || live) {
            Column(
                Modifier.fillMaxWidth().padding(Space.md),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                // Covering customer — leading person tone-tile + name (mirrors the
                // Delivery card's customer header).
                name?.let {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        Box(
                            Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
                            contentAlignment = Alignment.Center,
                        ) { MadarIcon("person.fill", tint = c.accent, size = IconSize.md) }
                        Text(it, style = Type.title(), color = c.textPrimary, modifier = Modifier.weight(1f))
                    }
                }
                if (live) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        MadarButton(t("waiter.add_round"), onAddRound, modifier = Modifier.weight(1f), variant = BtnVariant.OUTLINE, icon = "plus")
                        MadarButton(t("common.void"), onVoid, modifier = Modifier.weight(1f), variant = BtnVariant.GHOST, icon = "xmark")
                    }
                }
            }
        }
    }
}

/** Settle-tab ticket card (cashier) — the SAME status-tinted card shell as the
 *  waiter board / delivery card (status strip + bold-teal total), then a body with
 *  the covering + a "View order" / "Settle" action pair. Tapping the card body (or
 *  View) opens the shared order-details sheet; Settle opens the shared checkout. */
@Composable
private fun SettleTicketCard(
    ticket: TicketView,
    currency: String,
    onView: () -> Unit,
    onSettle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    val (statusFg, statusBg) = ticketStatusTint(ticket.status, c)
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(Radii.md)
    Column(
        modifier.fillMaxWidth()
            .elevation(Elevation.CARD, shape).clip(shape).background(c.surface)
            .border(1.dp, c.borderLight, shape)
            .pressScale(interaction)
            .clickable(interactionSource = interaction, indication = null) { onView() },
    ) {
        // Status-tinted header strip — dot + ref + state lead, money is the hero.
        Row(
            Modifier.fillMaxWidth().height(56.dp).background(statusBg).padding(horizontal = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(statusFg))
            Text(
                ticket.ticketRef ?: t("waiter.ticket"),
                color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Black, fontSize = 19.sp, maxLines = 1,
            )
            TicketStatusChip(ticket.status)
            if (ticket.queuedOffline) StatusChip(t("waiter.queued"), ChipTone.WARNING, icon = "tray.and.arrow.up")
            Spacer(Modifier.weight(1f))
            Box(
                Modifier.clip(RoundedCornerShape(Radii.sm)).background(c.accentBg)
                    .padding(horizontal = Space.md, vertical = 7.dp),
            ) {
                Text(Money.format(ticket.subtotalMinor, currency), style = Type.money(16.sp, FontWeight.Black), color = c.accent)
            }
        }
        Column(
            Modifier.fillMaxWidth().padding(Space.md),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            // Covering + item count — leading person tile (mirrors the delivery card).
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                Box(
                    Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
                    contentAlignment = Alignment.Center,
                ) { MadarIcon("person.fill", tint = c.accent, size = IconSize.md) }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    ticket.customerName?.takeIf { it.isNotBlank() }?.let {
                        Text(it, style = Type.title(), color = c.textPrimary, maxLines = 1)
                    }
                    Text("${ticket.lines.size} ${t("waiter.items")}", color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                MadarButton(t("order.view_order"), onView, modifier = Modifier.weight(1f), variant = BtnVariant.OUTLINE, icon = "list.bullet")
                MadarButton(t("waiter.settle"), onSettle, modifier = Modifier.weight(1f), icon = "checkmark.circle")
            }
        }
    }
}

/** Void-ticket confirmation as a branded bottom sheet (was a Material AlertDialog). */
@Composable
private fun ColumnScope.VoidSheetContent(ticket: TicketView, dismiss: () -> Unit, onConfirm: (String?) -> Unit) {
    val c = madarColors()
    var reason by remember { mutableStateOf("") }
    Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
        Text(t("waiter.void_title"), style = Type.h2(), color = c.textPrimary)
        ticket.ticketRef?.let { Text(it, style = Type.bodySm(), color = c.textSecondary) }
        MadarTextField(reason, { reason = it }, t("waiter.void_reason"), icon = "exclamationmark.bubble")
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            MadarButton(t("common.cancel"), dismiss, modifier = Modifier.weight(1f), variant = BtnVariant.GHOST)
            MadarButton(t("common.void"), { onConfirm(reason.ifBlank { null }) }, modifier = Modifier.weight(1f), variant = BtnVariant.DANGER, icon = "xmark")
        }
    }
}

