@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.OrderSummaryView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.EmptyState
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarButton
import app.madar.ui.MadarIcon
import app.madar.ui.MadarTextField
import app.madar.ui.Money
import app.madar.ui.Radii
import app.madar.ui.SelectableChip
import app.madar.ui.Space
import app.madar.ui.Type
import app.madar.ui.madarColors
import app.madar.ui.t
import kotlinx.coroutines.launch

// All-orders search — a history lookup ACROSS shifts (date range + status +
// teller), paginated. Closes the "operators can't look up a past-shift order"
// gap. Full-screen over the order screen; teller-only. Mirror of OrderSearchView.
@Composable
fun OrderSearchScreen(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    var status by remember { mutableStateOf<String?>(null) }   // null = all
    var teller by remember { mutableStateOf("") }
    var days by remember { mutableStateOf(7L) }                // 0 = all time

    fun run(reset: Boolean) {
        scope.launch { model.searchOrders(status, teller, null, if (days > 0) isoDaysAgo(days) else null, reset) }
    }
    LaunchedEffect(Unit) { run(true) }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // Header
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                MadarIcon("chevron.backward", tint = c.textPrimary, size = 18.dp, modifier = Modifier.clickable { model.showOrderSearch = false })
                Text(t("search.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 17.sp)
                Box(Modifier.weight(1f))
                if (model.orderSearchTotal > 0) {
                    Text("${model.orderSearchTotal}", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        // Filters
        Column(Modifier.fillMaxWidth().background(c.surface).padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                SelectableChip(t("search.date_24h"), days == 1L, { days = 1; run(true) })
                SelectableChip(t("search.date_7d"), days == 7L, { days = 7; run(true) })
                SelectableChip(t("search.date_30d"), days == 30L, { days = 30; run(true) })
                SelectableChip(t("order.all"), days == 0L, { days = 0; run(true) })
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                SelectableChip(t("order.all"), status == null, { status = null; run(true) })
                SelectableChip(t("history.completed"), status == "completed", { status = "completed"; run(true) })
                SelectableChip(t("history.voided"), status == "voided", { status = "voided"; run(true) })
            }
            Row(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { MadarTextField(teller, { teller = it }, t("search.teller_hint"), icon = "person") }
                MadarButton(t("search.title"), { run(true) }, fullWidth = false, icon = "magnifyingglass", loading = model.isSearchingOrders)
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))

        // Results
        when {
            model.isSearchingOrders && model.orderSearchResults.isEmpty() ->
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator(color = c.accent) }
            model.orderSearchResults.isEmpty() -> EmptyState("magnifyingglass", t("history.no_match"))
            else -> LazyColumn(
                Modifier.fillMaxSize(),
                contentPadding = PaddingValues(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                items(model.orderSearchResults, key = { it.id }) { o -> SearchResultRow(model, o, currency) }
                if (model.orderSearchHasMore) {
                    item("more") {
                        MadarButton(t("search.load_more"), { run(false) }, variant = BtnVariant.OUTLINE, loading = model.isSearchingOrders, icon = "arrow.down.circle")
                    }
                }
            }
        }
    }
}

@Composable
private fun SearchResultRow(model: AppModel, o: OrderSummaryView, currency: String) {
    val c = madarColors()
    val tone = when (o.status) {
        "voided" -> c.danger; "completed" -> c.success; "failed" -> c.danger; "queued" -> c.warning; else -> c.textSecondary
    }
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text("#${o.orderNumber ?: "—"}", style = Type.h3(), color = c.textPrimary)
            Text(model.fmtDateTime(o.createdAt), style = Type.bodySm(), color = c.textMuted)
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(Money.format(o.totalMinor, currency), style = Type.money(15.sp), color = c.textPrimary)
            Text("${o.status} · ${o.paymentLabel}", style = Type.labelSm(), color = tone)
        }
    }
}
