package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.LocalMadarFont
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.MadarIcon
import app.madar.ui.madarColors
import app.madar.ui.t

// Unified "Orders" surface (teller): delivery + waiter open-tickets in ONE place,
// two tabs, fed by the ONE session-level SSE. Replaces the separate delivery and
// settle-tickets screens. Both tabs are live (delivery → deliveryTick, tickets →
// ticketTick) and new incoming work pings + notifies via the core's realtime
// alert path, so a waiter firing on another device reaches the teller instantly.
@Composable
fun IncomingScreen(model: AppModel) {
    val c = madarColors()
    var tab by remember { mutableStateOf(model.incomingTab) }

    // Load both lists on entry so the tab badges are populated immediately (each
    // body also reloads itself + keys on its own live tick once selected).
    LaunchedEffect(Unit) { model.loadDeliveryOrders(); model.loadOpenTickets() }

    val deliveryCount = model.deliveryOrders.size
    val ticketCount = model.openTickets.count { it.status == "open" || it.status == "ready" }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            // Header: back + title.
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MadarIcon("chevron.backward", tint = c.textPrimary, size = 17.dp,
                    modifier = Modifier.clickable { model.showIncoming = false })
                Spacer(Modifier.width(Space.md))
                Text(t("incoming.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 17.sp)
            }
            // Segmented tab bar with live per-tab counts.
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg).padding(bottom = Space.md),
                horizontalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                TabSeg(Modifier.weight(1f), t("delivery.title"), deliveryCount, tab == 0) { tab = 0; model.incomingTab = 0 }
                TabSeg(Modifier.weight(1f), t("waiter.title"), ticketCount, tab == 1) { tab = 1; model.incomingTab = 1 }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }
        // Body — `when` swaps the tab so each body's own LaunchedEffects (initial
        // load + live tick) run on (re)entry.
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (tab) {
                0 -> DeliveryBody(model)
                else -> TicketsSettleBody(model)
            }
        }
    }
}

@Composable
private fun TabSeg(modifier: Modifier, label: String, count: Int, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    Row(
        modifier.clip(RoundedCornerShape(Radii.sm))
            .background(if (active) c.accent else c.surfaceAlt)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onClick() }
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
        if (count > 0) {
            Spacer(Modifier.width(Space.sm))
            Box(
                Modifier.clip(RoundedCornerShape(Radii.sm))
                    .background(if (active) c.textOnAccent else c.accent)
                    .padding(horizontal = 7.dp, vertical = 1.dp),
            ) {
                Text("$count", color = if (active) c.accent else c.textOnAccent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
            }
        }
    }
}
