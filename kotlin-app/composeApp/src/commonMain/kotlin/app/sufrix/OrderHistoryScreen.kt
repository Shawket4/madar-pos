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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
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
import app.sufrix.core.OrderSummaryView
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// Order history — the current shift's orders: still-queued sales (Queued/Failed
// chip) + the server's synced orders. Full-screen over the order screen; tap a
// row for its totals. Mirror of the SwiftUI OrderHistoryView.
@Composable
fun OrderHistoryScreen(model: AppModel) {
    val c = sufrixColors()
    var expandedId by remember { mutableStateOf<String?>(null) }
    val currency = model.session?.currencyCode ?: ""
    LaunchedEffect(Unit) { model.loadHistory() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // ── Header ────────────────────────────────────────────────────────────
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Text("‹", color = c.textPrimary, fontSize = 26.sp, modifier = Modifier.clickable { model.showHistory = false })
                Text(t("history.title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
                Box(Modifier.weight(1f))
                if (model.isLoadingHistory && model.history.isNotEmpty()) {
                    CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp, modifier = Modifier.size(18.dp))
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        // ── Content ───────────────────────────────────────────────────────────
        if (model.history.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                if (model.isLoadingHistory) {
                    CircularProgressIndicator(color = c.accent)
                } else {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                        Text("▦", color = c.textMuted, fontSize = 40.sp)
                        Text(t("history.empty"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
                    }
                }
            }
        } else {
            LazyColumn(
                Modifier.fillMaxSize(),
                contentPadding = PaddingValues(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                items(model.history, key = { it.id }) { item ->
                    HistoryRow(item, currency, expandedId == item.id) {
                        expandedId = if (expandedId == item.id) null else item.id
                    }
                }
            }
        }
    }
}

@Composable
private fun HistoryRow(item: OrderSummaryView, currency: String, expanded: Boolean, onToggle: () -> Unit) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).clickable { onToggle() }.padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(item.orderNumber?.let { "#$it" } ?: t("history.order"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                Text(timeOf(item.createdAt), color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)
            }
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(Money.format(item.totalMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                StatusChipFor(item)
            }
        }
        if (expanded) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            DetailRow(t("order.subtotal"), Money.format(item.subtotalMinor, currency))
            DetailRow(t("order.tax"), Money.format(item.taxMinor, currency))
            Row(Modifier.fillMaxWidth()) {
                Text(item.paymentLabel, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
                Box(Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun StatusChipFor(item: OrderSummaryView) {
    when {
        item.status == "failed" -> StatusChip(t("history.failed"), ChipTone.DANGER)
        item.queued -> StatusChip(t("history.queued"), ChipTone.WARNING)
        item.status == "voided" -> StatusChip(t("history.voided"), ChipTone.DANGER)
    }
}

@Composable
private fun DetailRow(label: String, value: String) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp)
        Box(Modifier.weight(1f))
        Text(value, color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
    }
}

/** rfc3339 → "HH:MM". */
private fun timeOf(rfc3339: String): String {
    val i = rfc3339.indexOf('T')
    return if (i >= 0) rfc3339.substring(i + 1).take(5) else rfc3339
}
