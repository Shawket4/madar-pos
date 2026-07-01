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
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.Opacity
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.Type
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.pressScale
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
        // Raised header surface: back + title, then the live segmented tab bar.
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MadarIcon("chevron.backward", tint = c.textPrimary, size = IconSize.xl,
                    modifier = Modifier.clickable { model.showIncoming = false })
                Spacer(Modifier.width(Space.md))
                Text(
                    t("incoming.title"),
                    style = Type.h3().copy(fontWeight = FontWeight.Black, fontSize = 17.sp),
                    color = c.textPrimary,
                )
            }
            // Segmented tab bar (teal active fill) with live per-tab count badges.
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg).padding(bottom = Space.md)
                    .clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt).padding(Space.xs),
                horizontalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                IncomingTab(t("delivery.title"), deliveryCount, tab == 0, Modifier.weight(1f)) {
                    tab = 0; model.incomingTab = 0
                }
                IncomingTab(t("waiter.title"), ticketCount, tab == 1, Modifier.weight(1f)) {
                    tab = 1; model.incomingTab = 1
                }
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

/** One segment of the Incoming tab bar — label + an optional count pill, teal
 *  fill when active. Mirrors the held-orders tab idiom (active = on-accent count
 *  pill, idle = surface). The parent owns width/weight via [modifier]. */
@Composable
private fun IncomingTab(label: String, count: Int, active: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    val fg = if (active) c.textOnAccent else c.textSecondary
    Row(
        modifier
            .pressScale(interaction)
            .clip(RoundedCornerShape(Radii.xs))
            .background(if (active) c.accent else Color.Transparent)
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(horizontal = Space.md, vertical = Space.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(
            label, color = fg,
            style = Type.title().copy(fontWeight = if (active) FontWeight.Bold else FontWeight.SemiBold),
        )
        if (count > 0) {
            Spacer(Modifier.width(Space.sm))
            Box(
                Modifier.clip(CircleShape)
                    .background(if (active) c.textOnAccent.copy(alpha = Opacity.border) else c.surface)
                    .padding(horizontal = Space.xs + 2.dp, vertical = 1.dp),
            ) {
                Text("$count", color = fg, style = Type.labelSm().copy(fontWeight = FontWeight.Bold))
            }
        }
    }
}
