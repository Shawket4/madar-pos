@file:OptIn(ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.KdsTicketView
import app.madar.ui.ChipTone
import app.madar.ui.LocalMadarFont
import app.madar.ui.NoticeBanner
import app.madar.ui.Space
import app.madar.ui.MadarIcon
import app.madar.ui.madarColors
import app.madar.ui.t
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

// Kitchen Display — the full-screen board a `kitchen`-role device shows. Subscribes
// to the branch's `kitchen` topic (one SSE), lists outstanding tickets, bumps lines
// tap-by-tap. Mirror of the SwiftUI KitchenDisplayView.
@Composable
fun KitchenDisplayScreen(model: AppModel, stationId: String) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val stationName = model.kdsStations.firstOrNull { it.id == stationId }?.name ?: t("kds.title")

    LaunchedEffect(Unit) {
        model.loadKdsStations()
        model.loadKds()
        // Live kitchen events arrive on the ONE session-level subscription (started
        // at login; the core picks the kitchen topic for a KDS device) — no per-screen
        // subscription here.
    }
    // Reload on each kitchen event + a slow safety-net poll.
    LaunchedEffect(model.kitchenTick) { model.loadKds() }
    LaunchedEffect(Unit) {
        while (isActive) { delay(60_000); model.loadKds() }
    }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // Header
        Row(
            Modifier.fillMaxWidth().background(c.surface).padding(Space.lg, Space.md),
            verticalAlignment = Alignment.CenterVertically
        ) {
            MadarIcon("fork.knife", tint = c.accent, size = 18.dp)
            Spacer(Modifier.width(Space.sm))
            Text(stationName, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 18.sp)
            Spacer(Modifier.width(Space.sm))
            Box(Modifier.size(8.dp).clip(CircleShape).background(if (model.realtimeConnected) c.success else c.textMuted))
            Spacer(Modifier.weight(1f))
            Text("${model.kdsTickets.size}", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            Spacer(Modifier.width(Space.md))
            MadarIcon("gearshape", tint = c.textSecondary, size = 18.dp, modifier = Modifier.clickable { model.showSettings = true })
        }
        if (!model.realtimeConnected) {
            NoticeBanner(t("kds.reconnecting"), tone = ChipTone.WARNING, icon = "wifi.slash")
        }
        if (model.kdsTickets.isEmpty()) {
            Column(
                Modifier.fillMaxSize(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(Space.md, Alignment.CenterVertically),
            ) {
                MadarIcon("checkmark.circle", tint = c.textMuted, size = 44.dp)
                Text(t("kds.all_clear"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
            }
        } else {
            LazyVerticalGrid(
                columns = GridCells.Adaptive(260.dp),
                contentPadding = PaddingValues(Space.lg),
                horizontalArrangement = Arrangement.spacedBy(Space.md),
                verticalArrangement = Arrangement.spacedBy(Space.md),
                modifier = Modifier.fillMaxSize()
            ) {
                items(model.kdsTickets, key = { it.id }) { ticket ->
                    KdsTicketCard(model, ticket, scope)
                }
            }
        }
    }
    if (model.showSettings) SettingsScreen(model)
}

@Composable
private fun KdsTicketCard(model: AppModel, ticket: KdsTicketView, scope: kotlinx.coroutines.CoroutineScope) {
    val c = madarColors()
    val ready = ticket.status == "ready"
    val ageMinutes = minutesSince(ticket.createdAt)
    // Age coloring: fresh accent → amber at 5m → red at 10m; ready is always
    // success. This SLA cue (and the matching escalating border) was on Swift but
    // silently dropped on Kotlin — the core's whole point on a KDS board.
    val ageTone = when {
        ready -> c.success
        ageMinutes >= 10 -> c.danger
        ageMinutes >= 5 -> c.warning
        else -> c.accent
    }
    val shape = RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth()
            .clip(shape)
            .background(c.surface)
            .border(if (ready) 2.dp else 1.dp, ageTone.copy(alpha = if (ready) 0.6f else 0.25f), shape)
            .padding(Space.md)
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(
                ticket.tableLabel ?: ticket.kitchenRef ?: "#${ticket.roundNumber}",
                color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp
            )
            Spacer(Modifier.weight(1f))
            if (ticket.sourceType == "open_ticket") {
                Text(t("kds.waiter"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
                Spacer(Modifier.width(Space.sm))
            }
            // Age SLA badge — escalates with the border.
            Text("${ageMinutes}m", color = ageTone, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        Spacer(Modifier.height(Space.sm))
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        Spacer(Modifier.height(Space.xs))
        ticket.items.forEach { line ->
            Row(
                Modifier.fillMaxWidth().clickable {
                    scope.launch { if (line.bumped) model.unbumpKdsItem(line.id) else model.bumpKdsItem(line.id) }
                }.padding(vertical = 4.dp),
                verticalAlignment = Alignment.Top
            ) {
                MadarIcon(if (line.bumped) "checkmark.circle" else "circle", tint = if (line.bumped) c.success else c.textMuted, size = 18.dp)
                Spacer(Modifier.width(Space.sm))
                Column(Modifier.weight(1f)) {
                    Text(
                        "${line.qty}× ${line.name}" + (line.sizeLabel?.let { " · $it" } ?: ""),
                        color = if (line.bumped) c.textMuted else c.textPrimary,
                        fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 14.sp,
                        textDecoration = if (line.bumped) TextDecoration.LineThrough else TextDecoration.None
                    )
                    if (line.modifiers.isNotEmpty()) {
                        Text(line.modifiers.joinToString(", "), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
                    }
                    line.notes?.takeIf { it.isNotBlank() }?.let {
                        Text(it, color = c.warning, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                    }
                }
                // Per-line station label (expo / all-station board) — the core
                // populates stationName but neither host rendered it.
                line.stationName?.takeIf { it.isNotBlank() }?.let {
                    Spacer(Modifier.width(Space.sm))
                    Text(it.uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 10.sp)
                }
            }
        }
    }
}
