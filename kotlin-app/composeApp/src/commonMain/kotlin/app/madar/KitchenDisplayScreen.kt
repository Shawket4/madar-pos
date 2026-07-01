@file:OptIn(ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
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
import app.madar.ui.Elevation
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.MadarIcon
import app.madar.ui.elevation
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.t
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import androidx.compose.runtime.LaunchedEffect

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

    Box(Modifier.fillMaxSize()) {
    Column(Modifier.fillMaxSize().background(c.bg)) {
        KitchenHeader(
            stationName = stationName,
            ticketCount = model.kdsTickets.size,
            connected = model.realtimeConnected,
            onSettings = { model.showSettings = true },
            modifier = Modifier.fillMaxWidth(),
        )
        if (!model.realtimeConnected) {
            Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                NoticeBanner(t("kds.reconnecting"), tone = ChipTone.WARNING, icon = "wifi.slash")
            }
        }
        if (model.kdsTickets.isEmpty()) {
            KdsEmptyState(Modifier.fillMaxSize())
        } else {
            LazyVerticalGrid(
                columns = GridCells.Adaptive(260.dp),
                contentPadding = PaddingValues(Space.lg),
                horizontalArrangement = Arrangement.spacedBy(Space.md),
                verticalArrangement = Arrangement.spacedBy(Space.md),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(model.kdsTickets, key = { it.id }) { ticket ->
                    KdsTicketCard(ticket, Modifier.fillMaxWidth(),
                        onBump = { lineId, bumped ->
                            scope.launch { if (bumped) model.unbumpKdsItem(lineId) else model.bumpKdsItem(lineId) }
                        })
                }
            }
        }
    }
        // Settings opens as an overlay — kept INSIDE the root Box so it draws ON
        // TOP of the board. It was a bare sibling after the Column, so it fell
        // behind the full-size board (the "options hidden behind the screen" bug).
        if (model.showSettings) SettingsScreen(model)
    }
}

// ── Header ───────────────────────────────────────────────────────────────────────
// Clean, confident board header: a leading teal tone-tile behind the station glyph,
// the bold station name, a live-connection dot, and the outstanding-ticket count.
@Composable
private fun KitchenHeader(
    stationName: String,
    ticketCount: Int,
    connected: Boolean,
    onSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    Column(modifier.background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Box(
                Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
                contentAlignment = Alignment.Center,
            ) {
                MadarIcon("fork.knife", tint = c.accent, size = IconSize.lg)
            }
            Text(
                stationName, color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Black, fontSize = 20.sp,
            )
            Box(Modifier.size(8.dp).clip(CircleShape).background(if (connected) c.success else c.textMuted))
            Box(Modifier.weight(1f))
            if (ticketCount > 0) StatusChip("$ticketCount", ChipTone.ACCENT)
            MadarIcon(
                "gearshape", tint = c.textSecondary, size = IconSize.lg,
                modifier = Modifier.padding(start = Space.xs).clickable { onSettings() },
            )
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

// ── All-clear empty state ──────────────────────────────────────────────────────
@Composable
private fun KdsEmptyState(modifier: Modifier = Modifier) {
    val c = madarColors()
    Column(
        modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.md, Alignment.CenterVertically),
    ) {
        Box(
            Modifier.size(72.dp).clip(RoundedCornerShape(Radii.lg)).background(c.successBg),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon("checkmark.circle", tint = c.success, size = 36.dp)
        }
        Text(
            t("kds.all_clear"), color = c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = FontWeight.SemiBold, fontSize = 16.sp,
        )
    }
}

// ── Ticket card ────────────────────────────────────────────────────────────────
// A raised card with an age-TINTED header strip so a cook reads urgency from across
// the kitchen: fresh teal → amber (5m) → red (10m); a ready ticket goes green and
// gets a heavier border. The header is a FIXED height, so every card's item list
// starts at the SAME y regardless of table label or waiter chip.
@Composable
private fun KdsTicketCard(
    ticket: KdsTicketView,
    modifier: Modifier = Modifier,
    onBump: (lineId: String, bumped: Boolean) -> Unit,
) {
    val c = madarColors()
    val ready = ticket.status == "ready"
    val ageMinutes = minutesSince(ticket.createdAt)
    val ageFg = when {
        ready -> c.success
        ageMinutes >= 10 -> c.danger
        ageMinutes >= 5 -> c.warning
        else -> c.accent
    }
    val ageBg = when {
        ready -> c.successBg
        ageMinutes >= 10 -> c.dangerBg
        ageMinutes >= 5 -> c.warningBg
        else -> c.accentBg
    }
    val shape = RoundedCornerShape(Radii.md)
    Column(
        modifier
            .elevation(Elevation.CARD, shape)
            .clip(shape)
            .background(c.surface)
            .border(if (ready) 2.dp else 1.dp, if (ready) ageFg.copy(alpha = 0.6f) else c.borderLight, shape),
    ) {
        // Age-tinted header strip — fixed height aligns every card's first item.
        Row(
            Modifier.fillMaxWidth().height(54.dp).background(ageBg).padding(horizontal = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Text(
                ticket.tableLabel ?: ticket.kitchenRef ?: "#${ticket.roundNumber}",
                color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Black, fontSize = 19.sp, maxLines = 1,
            )
            if (ticket.sourceType == "open_ticket") {
                MadarIcon("person.fill", tint = ageFg, size = IconSize.md)
            }
            Box(Modifier.weight(1f))
            Text("${ageMinutes}m", color = ageFg, style = Type.money(18.sp, FontWeight.Black))
        }
        Column(Modifier.fillMaxWidth().padding(horizontal = Space.md, vertical = Space.xs)) {
            ticket.items.forEach { line ->
                KdsLineRow(line, onBump = { onBump(line.id, line.bumped) }, modifier = Modifier.fillMaxWidth())
            }
        }
    }
}

// ── Bumpable line ──────────────────────────────────────────────────────────────
// One tappable line: a check toggle, the qty × name (+ size), modifiers, and an
// optional kitchen note (warning-tinted). Bumped lines mute + strike through. The
// per-line station label (expo board) pins to the trailing edge.
@Composable
private fun KdsLineRow(
    line: app.madar.core.KdsLineView,
    onBump: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Row(
        modifier
            .pressScale(interaction)
            .clip(RoundedCornerShape(Radii.xs))
            .clickable(interactionSource = interaction, indication = null) { onBump() }
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        MadarIcon(
            if (line.bumped) "checkmark.circle.fill" else "circle",
            tint = if (line.bumped) c.success else c.textMuted, size = 22.dp,
        )
        Spacer(Modifier.width(Space.sm))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                "${line.qty}× ${line.name}" + (line.sizeLabel?.let { " · $it" } ?: ""),
                color = if (line.bumped) c.textMuted else c.textPrimary,
                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp,
                textDecoration = if (line.bumped) TextDecoration.LineThrough else TextDecoration.None,
            )
            if (line.modifiers.isNotEmpty()) {
                Text(
                    line.modifiers.joinToString(", "), color = c.textSecondary,
                    fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 12.sp,
                )
            }
            line.notes?.takeIf { it.isNotBlank() }?.let {
                Text(
                    it, color = c.warning, fontFamily = LocalMadarFont.current,
                    fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                )
            }
        }
        // Per-line station label (expo / all-station board) — the core populates
        // stationName but neither host rendered it before.
        line.stationName?.takeIf { it.isNotBlank() }?.let {
            Spacer(Modifier.width(Space.sm))
            Text(
                it.uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Bold, fontSize = 10.sp,
            )
        }
    }
}
