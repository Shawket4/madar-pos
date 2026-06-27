@file:OptIn(ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.TicketView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.LocalMadarFont
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Space
import app.madar.ui.MadarButton
import app.madar.ui.MadarIcon
import app.madar.ui.MadarSheet
import app.madar.ui.MadarTextField
import app.madar.ui.SectionHeader
import app.madar.ui.SelectableChip
import app.madar.ui.SheetSize
import app.madar.ui.Type
import app.madar.ui.madarColors
import app.madar.ui.t
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
            Row(Modifier.fillMaxWidth().background(c.surface).padding(Space.lg, Space.md), verticalAlignment = Alignment.CenterVertically) {
                MadarIcon("chevron.backward", tint = c.textPrimary, size = 18.dp, modifier = Modifier.clickable { model.showTickets = false })
                Spacer(Modifier.width(Space.sm))
                Text(t("waiter.tickets"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 17.sp)
                Spacer(Modifier.weight(1f))
                MadarIcon("arrow.clockwise", tint = c.textSecondary, size = 18.dp,
                    modifier = Modifier.clickable { scope.launch { model.loadOpenTickets() } })
            }
            model.error?.let { NoticeBanner(it, tone = ChipTone.WARNING, icon = "exclamationmark.circle") }

            if (model.openTickets.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text(t("waiter.no_tickets"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                }
            } else {
                LazyColumn(contentPadding = PaddingValues(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    items(model.openTickets, key = { it.id }) { ticket ->
                        TicketRow(ticket, currency,
                            // "Add round": target this ticket, return to the order screen
                            // (its Fire button becomes "Add round" and fires into it).
                            onAddRound = { model.clearCart(); model.activeTicketId = ticket.id; model.showTickets = false },
                            onVoid = { voiding = ticket })
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
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    var settling by remember { mutableStateOf<TicketView?>(null) }
    val settleable = model.openTickets.filter { it.status == "open" || it.status == "ready" }

    LaunchedEffect(Unit) { model.loadOpenTickets() }
    LaunchedEffect(model.ticketTick) { model.loadOpenTickets() }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxSize()) {
            model.error?.let { NoticeBanner(it, tone = ChipTone.WARNING, icon = "exclamationmark.circle") }
            if (settleable.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                        MadarIcon("tray", tint = c.textMuted, size = 40.dp)
                        Text(t("waiter.no_tickets"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                    }
                }
            } else {
                LazyColumn(contentPadding = PaddingValues(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    items(settleable, key = { it.id }) { ticket ->
                        Row(
                            Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(c.surface)
                                .border(1.dp, c.border, RoundedCornerShape(12.dp))
                                .clickable { settling = ticket }.padding(Space.md),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(ticket.ticketRef ?: t("waiter.ticket"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp)
                            ticket.customerName?.takeIf { it.isNotBlank() }?.let {
                                Spacer(Modifier.width(Space.sm))
                                Text(it, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
                            }
                            Spacer(Modifier.width(Space.sm))
                            Text(t("ticket.status.${ticket.status}"), color = if (ticket.status == "ready") c.success else c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
                            Spacer(Modifier.weight(1f))
                            Text(Money.format(ticket.subtotalMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                            Spacer(Modifier.width(Space.sm))
                            MadarIcon("chevron.forward", tint = c.textMuted, size = 14.dp)
                        }
                    }
                }
            }
        }
        // Settle sheet — branded MadarSheet (was a Material AlertDialog that hid
        // the line items). Stay on the list after settling (the ticket drops out
        // on reload).
        settling?.let { ticket ->
            MadarSheet(onDismiss = { settling = null }, size = SheetSize.LARGE, maxWidth = 560.dp) { dismiss ->
                SettleSheetContent(model, ticket, currency, dismiss)
            }
        }
    }
}

@Composable
private fun TicketRow(ticket: TicketView, currency: String, onAddRound: () -> Unit, onVoid: () -> Unit) {
    val c = madarColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(12.dp)).padding(Space.md)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(ticket.ticketRef ?: t("waiter.ticket"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp)
            Spacer(Modifier.width(Space.sm))
            Text(t("ticket.status.${ticket.status}"), color = if (ticket.status == "ready") c.success else c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
            if (ticket.queuedOffline) {
                Spacer(Modifier.width(Space.sm))
                Text(t("waiter.queued"), color = c.warning, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
            }
            Spacer(Modifier.weight(1f))
            Text(Money.format(ticket.subtotalMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp)
        }
        ticket.customerName?.takeIf { it.isNotBlank() }?.let {
            Text(it, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
        }
        if (ticket.status == "open" || ticket.status == "ready") {
            Spacer(Modifier.height(Space.sm))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                MadarButton(t("waiter.add_round"), onAddRound, modifier = Modifier.weight(1f), variant = BtnVariant.OUTLINE, icon = "plus")
                MadarButton(t("common.void"), onVoid, modifier = Modifier.weight(1f), variant = BtnVariant.GHOST, icon = "xmark")
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
        ticket.ticketRef?.let { Text(it, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp) }
        MadarTextField(reason, { reason = it }, t("waiter.void_reason"), icon = "exclamationmark.bubble")
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            MadarButton(t("common.cancel"), dismiss, modifier = Modifier.weight(1f), variant = BtnVariant.GHOST)
            MadarButton(t("common.void"), { onConfirm(reason.ifBlank { null }) }, modifier = Modifier.weight(1f), variant = BtnVariant.DANGER, icon = "xmark")
        }
    }
}

/** Settle a ticket — a branded bottom sheet that REVIEWS the line items + total
 *  before charging (was a Material AlertDialog showing only a bare number, so the
 *  cashier settled blind). Mirrors the Swift SettleSheet. */
@Composable
private fun ColumnScope.SettleSheetContent(model: AppModel, ticket: TicketView, currency: String, dismiss: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var methodId by remember { mutableStateOf(model.paymentMethods.firstOrNull()?.id) }
    Column(Modifier.fillMaxSize().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
        Text(ticket.ticketRef ?: t("waiter.ticket"), style = Type.h2(), color = c.textPrimary)

        // Line items — what the cashier is charging for (strikethrough on voided).
        LazyColumn(Modifier.weight(1f, fill = false), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            items(ticket.lines) { line ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "${line.qty}× ${line.name}",
                        color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp,
                        textDecoration = if (line.voided) androidx.compose.ui.text.style.TextDecoration.LineThrough else null,
                        modifier = Modifier.weight(1f),
                    )
                    Text(Money.format(line.lineTotalMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                }
            }
        }

        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        // Total
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(t("tender.total"), style = Type.title(), color = c.textSecondary)
            Spacer(Modifier.weight(1f))
            Text(Money.format(ticket.subtotalMinor, currency), style = Type.moneyLg(), color = c.textPrimary)
        }

        // Payment method
        SectionHeader(t("tender.method"))
        androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            model.paymentMethods.forEach { pm ->
                SelectableChip(pm.name, isSelected = methodId == pm.id, onTap = { methodId = pm.id })
            }
        }

        MadarButton(t("waiter.settle"), {
            val id = methodId ?: return@MadarButton
            scope.launch { if (model.settleTicket(ticket.id, id, null)) dismiss() }
        }, loading = model.isBusy, enabled = methodId != null, icon = "checkmark.circle")
    }
}
