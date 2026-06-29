package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.OutboxItemView
import app.madar.ui.ChipTone
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.MadarColors
import app.madar.ui.LocalMadarFont
import app.madar.ui.backGlyph
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.fillMaxHeight

// Sync center — visibility into the durable outbox: queued / in-flight / failed
// (with the error). Retry requeues every failed command; a teller can discard a
// dead one. Full-screen over the order screen. Mirror of the SwiftUI SyncView.
@Composable
fun SyncScreen(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { model.loadOutbox() }
    val hasFailed = model.outbox.any { it.status == "dead" }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // ── Header ────────────────────────────────────────────────────────────
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                MadarIcon("chevron.backward", tint = c.textPrimary, size = 17.dp, modifier = Modifier.clickable { model.showSync = false })
                Text(t("sync.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 17.sp)
                Box(Modifier.weight(1f))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.lg),
                ) {
                    // Retry requeues only the FAILED (dead) rows, so it only appears
                    // when there's something dead to resurrect.
                    if (hasFailed) {
                        Row(
                            Modifier.clickable { scope.launch { model.retryOutbox() } },
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            MadarIcon("arrow.clockwise", tint = c.accent, size = IconSize.sm)
                            Text(t("sync.retry"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                        }
                    }
                    // "Sync now" force-pushes every QUEUED command (not just dead ones)
                    // — the manual escape hatch when the queue isn't draining on its
                    // own. Visible whenever anything is waiting to sync.
                    if (model.outbox.isNotEmpty()) {
                        Row(
                            Modifier
                                .clip(RoundedCornerShape(Radii.pill))
                                .background(c.accent)
                                .clickable(enabled = !model.isPushing) { scope.launch { model.syncNow() } }
                                .padding(horizontal = Space.md, vertical = 7.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            if (model.isPushing) {
                                CircularProgressIndicator(color = c.textOnAccent, strokeWidth = 2.dp, modifier = Modifier.size(14.dp))
                            } else {
                                MadarIcon("icloud.and.arrow.up", tint = c.textOnAccent, size = IconSize.sm)
                            }
                            Text(
                                if (model.isPushing) t("sync.pushing") else t("sync.push"),
                                color = c.textOnAccent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 13.sp,
                            )
                        }
                    }
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        // ── Content ───────────────────────────────────────────────────────────
        if (model.outbox.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    MadarIcon("checkmark.circle", tint = c.success, size = 40.dp)
                    Text(t("sync.empty"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 14.sp)
                }
            }
        } else {
            // One surface card; rows separated by hairlines (matches Swift / Flutter
            // _EntryGroupCard) — not per-row cards — capped + centered on tablet.
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Column(
                    Modifier.widthIn(max = 560.dp).fillMaxWidth()
                        .clip(RoundedCornerShape(Radii.md)).background(c.surface)
                        .border(1.dp, c.border, RoundedCornerShape(Radii.md)),
                ) {
                    model.outbox.forEachIndexed { index, item ->
                        if (index > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
                        SyncRow(model, item)
                    }
                }
            }
        }
    }
}

@Composable
private fun SyncRow(model: AppModel, item: OutboxItemView) {
    val c = madarColors()
    val tone = statusTone(item.status)
    Row(
        Modifier.fillMaxWidth().padding(start = Space.lg, end = Space.md, top = Space.md, bottom = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        // Leading op-type tile — mirrors SwiftUI: 38×38, Radii.xs, tone-tinted bg + op glyph.
        Box(
            Modifier.size(38.dp).clip(RoundedCornerShape(Radii.xs)).background(toneBg(tone, c)),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon(opGlyph(item.opType, item.status), tint = toneFg(tone, c), size = IconSize.lg)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(opLabel(item.opType), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1)
            val err = item.lastError
            if (!err.isNullOrEmpty()) {
                Text(err, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp, maxLines = 2)
            } else if (item.attempts > 0) {
                Text("${item.attempts} ${t("sync.attempts")}", color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp)
            }
        }
        StatusChip(statusLabel(item.status), statusTone(item.status))
        if (item.status == "dead") {
            MadarIcon("trash", tint = c.danger, size = 14.dp, modifier = Modifier.clickable { model.discardOutboxItem(item.id) })
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

// Tone bg/fg — ChipTone.bg/fg in Components.kt are private, so resolve the two
// tones the outbox uses (INFO / DANGER) locally, identical to StatusChip's mapping.
private fun toneBg(tone: ChipTone, c: MadarColors): Color =
    if (tone == ChipTone.DANGER) c.dangerBg else c.navyBg

private fun toneFg(tone: ChipTone, c: MadarColors): Color =
    if (tone == ChipTone.DANGER) c.danger else c.navy

@Composable
private fun statusLabel(status: String): String = when (status) {
    "dead" -> t("sync.failed")
    "inflight" -> t("sync.sending")
    else -> t("sync.queued")
}

private fun statusTone(status: String): ChipTone = if (status == "dead") ChipTone.DANGER else ChipTone.INFO
