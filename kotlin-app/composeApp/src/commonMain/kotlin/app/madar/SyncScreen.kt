package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.OutboxItemView
import app.madar.ui.ChipTone
import app.madar.ui.Elevation
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarIcon
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.elevation
import app.madar.ui.StatusChip
import app.madar.ui.bg
import app.madar.ui.fg
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.t
import kotlinx.coroutines.launch

// Sync center — visibility into the durable outbox: queued / in-flight / failed
// rows (with the error). Retry requeues every failed command; a teller can
// discard a dead one. Full-screen over the order screen. Mirror of the SwiftUI
// SyncView.
@Composable
fun SyncScreen(model: AppModel) {
    val c = madarColors()
    LaunchedEffect(Unit) { model.loadOutbox() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        SyncHeader(model, Modifier.fillMaxWidth())

        if (model.outbox.isEmpty()) {
            SyncEmptyState(Modifier.fillMaxSize())
        } else {
            // One surface card; rows separated by hairlines (matches Swift / Flutter
            // _EntryGroupCard) — not per-row cards — capped + centered on tablet.
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Column(
                    Modifier.widthIn(max = 560.dp).fillMaxWidth()
                        .elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
                        .clip(RoundedCornerShape(Radii.md)).background(c.surface)
                        .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)),
                ) {
                    model.outbox.forEachIndexed { index, item ->
                        if (index > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
                        SyncRow(model, item, Modifier.fillMaxWidth())
                    }
                }
            }
        }
    }
}

// ── Header ────────────────────────────────────────────────────────────────────
// Clean bold title with the back affordance, plus the two queue actions (Retry
// the failed rows, force-push everything queued).
@Composable
private fun SyncHeader(model: AppModel, modifier: Modifier = Modifier) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val backInteraction = remember { MutableInteractionSource() }
    // Retry requeues only the FAILED (dead) rows, so it only appears when there's
    // something dead to resurrect.
    val hasFailed = model.outbox.any { it.status == "dead" }

    Column(modifier.background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            MadarIcon(
                "chevron.backward", tint = c.textPrimary, size = 17.dp,
                modifier = Modifier.pressScale(backInteraction)
                    .clickable(interactionSource = backInteraction, indication = null) { model.showSync = false },
            )
            // Leading teal tone-tile behind the sync glyph — matches the confident
            // Kitchen/Order header (accentBg + accent icon, 34×34, Radii.sm).
            Box(
                Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
                contentAlignment = Alignment.Center,
            ) {
                MadarIcon("arrow.triangle.2.circlepath", tint = c.accent, size = IconSize.lg)
            }
            Text(t("sync.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
            Box(Modifier.weight(1f))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.lg),
            ) {
                if (hasFailed) {
                    SyncRetryButton { scope.launch { model.retryOutbox() } }
                }
                // "Sync now" force-pushes every QUEUED command (not just dead ones)
                // — the manual escape hatch when the queue isn't draining on its
                // own. Visible whenever anything is waiting to sync.
                if (model.outbox.isNotEmpty()) {
                    SyncNowButton(pushing = model.isPushing) { scope.launch { model.syncNow() } }
                }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

/** Requeue-the-failed action — a quiet accent text button. */
@Composable
private fun SyncRetryButton(onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.pressScale(interaction)
            .clickable(interactionSource = interaction, indication = null) { onClick() },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        MadarIcon("arrow.clockwise", tint = c.accent, size = IconSize.sm)
        Text(t("sync.retry"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

/** Force-push-the-queue action — the teal pill CTA; spins + disables while pushing. */
@Composable
private fun SyncNowButton(pushing: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.pressScale(interaction)
            .clip(RoundedCornerShape(Radii.pill))
            .background(c.accent)
            .clickable(enabled = !pushing, interactionSource = interaction, indication = null) { onClick() }
            .padding(horizontal = Space.md, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (pushing) {
            CircularProgressIndicator(color = c.textOnAccent, strokeWidth = 2.dp, modifier = Modifier.size(14.dp))
        } else {
            MadarIcon("icloud.and.arrow.up", tint = c.textOnAccent, size = IconSize.sm)
        }
        Text(
            if (pushing) t("sync.pushing") else t("sync.push"),
            color = c.textOnAccent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 13.sp,
        )
    }
}

// ── Empty state ─────────────────────────────────────────────────────────────────
// Nothing waiting to sync — a reassuring success mark.
@Composable
private fun SyncEmptyState(modifier: Modifier = Modifier) {
    val c = madarColors()
    Box(modifier, contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Box(
                Modifier.size(72.dp).clip(RoundedCornerShape(Radii.lg)).background(c.successBg),
                contentAlignment = Alignment.Center,
            ) {
                MadarIcon("checkmark.circle", tint = c.success, size = 36.dp)
            }
            Text(t("sync.empty"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
        }
    }
}

// ── Outbox row ──────────────────────────────────────────────────────────────────
@Composable
private fun SyncRow(model: AppModel, item: OutboxItemView, modifier: Modifier = Modifier) {
    val c = madarColors()
    val tone = statusTone(item.status)
    val discardInteraction = remember { MutableInteractionSource() }
    Row(
        modifier.heightIn(min = 68.dp).padding(start = Space.lg, end = Space.md, top = Space.md, bottom = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        // Leading op-type tile — mirrors SwiftUI: 40×40, Radii.sm, tone-tinted bg + op glyph.
        Box(
            Modifier.size(40.dp).clip(RoundedCornerShape(Radii.sm)).background(tone.bg(c)),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon(opGlyph(item.opType, item.status), tint = tone.fg(c), size = IconSize.lg)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(opLabel(item.opType), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1)
            val err = item.lastError
            if (!err.isNullOrEmpty()) {
                Text(err, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp, maxLines = 2)
            } else if (item.attempts > 0) {
                Text("${item.attempts} ${t("sync.attempts")}", color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
            }
        }
        StatusChip(statusLabel(item.status), tone)
        if (item.status == "dead") {
            MadarIcon(
                "trash", tint = c.danger, size = IconSize.md,
                modifier = Modifier.pressScale(discardInteraction)
                    .clip(RoundedCornerShape(Radii.xs))
                    .clickable(interactionSource = discardInteraction, indication = null) { model.discardOutboxItem(item.id) }
                    .padding(Space.xs),
            )
        }
    }
}

@Composable
private fun opLabel(op: String): String = when (op) {
    "open_shift" -> t("sync.op_open_shift")
    "close_shift" -> t("sync.op_close_shift")
    "create_order" -> t("sync.op_create_order")
    else -> op
}

// Op glyph — mirrors SwiftUI `opIcon` (dead → warning mark, else per op_type),
// using the existing Unicode-glyph convention (no Material-icons dependency).
private fun opGlyph(op: String, status: String): String {
    if (status == "dead") return "exclamationmark.circle"
    return when (op) {
        "open_shift" -> "play.circle"
        "close_shift" -> "lock"
        "create_order" -> "doc.text"
        else -> "arrow.clockwise"
    }
}

@Composable
private fun statusLabel(status: String): String = when (status) {
    "dead" -> t("sync.failed")
    "inflight" -> t("sync.sending")
    else -> t("sync.queued")
}

// Outbox tones map to the shared ChipTone scale: failed → DANGER, everything
// else (queued / in-flight) → INFO. The tile tint reuses ChipTone.bg/fg, so the
// leading icon and the status chip always read as the same tone.
private fun statusTone(status: String): ChipTone = if (status == "dead") ChipTone.DANGER else ChipTone.INFO
