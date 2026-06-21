package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.OutboxItemView
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.backGlyph
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.launch

// Sync center — visibility into the durable outbox: queued / in-flight / failed
// (with the error). Retry requeues every failed command; a teller can discard a
// dead one. Full-screen over the order screen. Mirror of the SwiftUI SyncView.
@Composable
fun SyncScreen(model: AppModel) {
    val c = sufrixColors()
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
                Text(backGlyph(), color = c.textPrimary, fontSize = 26.sp, modifier = Modifier.clickable { model.showSync = false })
                Text(t("sync.title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
                Box(Modifier.weight(1f))
                if (hasFailed) {
                    Text(
                        t("sync.retry"), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                        modifier = Modifier.clickable { scope.launch { model.retryOutbox() } },
                    )
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        // ── Content ───────────────────────────────────────────────────────────
        if (model.outbox.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    Text("✓", color = c.success, fontSize = 40.sp)
                    Text(t("sync.empty"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
                }
            }
        } else {
            LazyColumn(
                Modifier.fillMaxSize(),
                contentPadding = PaddingValues(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                items(model.outbox, key = { it.id }) { item -> SyncRow(model, item) }
            }
        }
    }
}

@Composable
private fun SyncRow(model: AppModel, item: OutboxItemView) {
    val c = sufrixColors()
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(opLabel(item.opType), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1)
            val err = item.lastError
            if (!err.isNullOrEmpty()) {
                Text(err, color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp, maxLines = 2)
            } else if (item.attempts > 0) {
                Text("${item.attempts} ${t("sync.attempts")}", color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)
            }
        }
        StatusChip(statusLabel(item.status), statusTone(item.status))
        if (item.status == "dead") {
            Text("✕", color = c.danger, fontSize = 16.sp, modifier = Modifier.clickable { model.discardOutboxItem(item.id) })
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

@Composable
private fun statusLabel(status: String): String = when (status) {
    "dead" -> t("sync.failed")
    "inflight" -> t("sync.sending")
    else -> t("sync.queued")
}

private fun statusTone(status: String): ChipTone = if (status == "dead") ChipTone.DANGER else ChipTone.INFO
