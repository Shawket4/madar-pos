@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package app.madar

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.background
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.OrderSummaryView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.Elevation
import app.madar.ui.EmptyState
import app.madar.ui.IconSize
import app.madar.ui.MadarButton
import app.madar.ui.MadarIcon
import app.madar.ui.MadarTextField
import app.madar.ui.Metric
import app.madar.ui.Money
import app.madar.ui.Radii
import app.madar.ui.ScreenHeader
import app.madar.ui.SelectableChip
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.elevation
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.screenHeaderBar
import app.madar.ui.t
import kotlinx.coroutines.launch

// All-orders search — a history lookup ACROSS shifts (date range + status +
// teller), paginated. Closes the "operators can't look up a past-shift order"
// gap. Full-screen over the order screen; teller-only. Mirror of OrderSearchView.
@Composable
fun OrderSearchScreen(model: AppModel) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val currency = model.session?.currencyCode ?: ""
    val exportedLabel = t("search.exported")
    var status by remember { mutableStateOf<String?>(null) }   // null = all
    var teller by remember { mutableStateOf("") }
    var days by remember { mutableStateOf(7L) }                // 0 = all time

    fun run(reset: Boolean) {
        scope.launch { model.searchOrders(status, teller, null, if (days > 0) isoDaysAgo(days) else null, reset) }
    }
    LaunchedEffect(Unit) { run(true) }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // Header — shared ScreenHeader on the standard bar chrome (mirror of
        // OrderSearchView). Trailing carries the result count + a copy-to-clipboard
        // CSV export action.
        Box(Modifier.screenHeaderBar()) {
            ScreenHeader(
                t("search.title"),
                onBack = { model.showOrderSearch = false },
            ) {
                if (model.orderSearchTotal > 0) {
                    Text("${model.orderSearchTotal}", style = Type.title(), color = c.textSecondary)
                }
                if (model.orderSearchResults.isNotEmpty()) {
                    val interaction = remember { MutableInteractionSource() }
                    Box(
                        Modifier.size(Metric.closeButton).pressScale(interaction)
                            .clip(RoundedCornerShape(Radii.sm)).background(c.accentBg)
                            .clickable(interactionSource = interaction, indication = null) {
                                clipboard.setText(AnnotatedString(ordersToCsv(model.orderSearchResults, currency)))
                                model.showToast(exportedLabel, ChipTone.SUCCESS, icon = "checkmark.circle.fill")
                            },
                        contentAlignment = Alignment.Center,
                    ) { MadarIcon("square.and.arrow.up", tint = c.accent, size = IconSize.lg) }
                }
            }
        }

        // Filters — date range, status, and a teller lookup, on a raised surface
        // block closed off with a hairline (matches the order screen chrome).
        Column(
            Modifier.fillMaxWidth().background(c.surface).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                SelectableChip(t("search.date_24h"), days == 1L, { days = 1; run(true) })
                SelectableChip(t("search.date_7d"), days == 7L, { days = 7; run(true) })
                SelectableChip(t("search.date_30d"), days == 30L, { days = 30; run(true) })
                SelectableChip(t("order.all"), days == 0L, { days = 0; run(true) })
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                SelectableChip(t("order.all"), status == null, { status = null; run(true) })
                SelectableChip(t("history.completed"), status == "completed", { status = "completed"; run(true) }, tone = ChipTone.SUCCESS)
                SelectableChip(t("history.voided"), status == "voided", { status = "voided"; run(true) }, tone = ChipTone.DANGER)
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
                items(model.orderSearchResults, key = { it.id }) { o ->
                    SearchResultRow(model.fmtDateTime(o.createdAt), o, currency, Modifier.fillMaxWidth())
                }
                if (model.orderSearchHasMore) {
                    item("more") {
                        MadarButton(t("search.load_more"), { run(false) }, variant = BtnVariant.OUTLINE, loading = model.isSearchingOrders, icon = "arrow.down.circle")
                    }
                }
            }
        }
    }
}

// Spreadsheet-friendly export of the current result page. RFC-4180 quoting so a
// payment label or status containing a comma can't shift columns.
private fun ordersToCsv(orders: List<OrderSummaryView>, currency: String): String {
    fun esc(s: String) = "\"" + s.replace("\"", "\"\"") + "\""
    val sb = StringBuilder("Order,Date,Total,Payment,Status\n")
    for (o in orders) {
        sb.append("#${o.orderNumber ?: ""},")
        sb.append(esc(o.createdAt)).append(",")
        sb.append(esc(Money.format(o.totalMinor, currency))).append(",")
        sb.append(esc(o.paymentLabel)).append(",")
        sb.append(esc(o.status)).append("\n")
    }
    return sb.toString()
}

/** Status → a tone-paired chip color (voided/failed = danger, completed =
 *  success, queued = warning, else neutral). */
private fun statusTone(status: String): ChipTone = when (status) {
    "voided", "failed" -> ChipTone.DANGER
    "completed" -> ChipTone.SUCCESS
    "queued" -> ChipTone.WARNING
    else -> ChipTone.NEUTRAL
}

/** One order result card: number + timestamp on the leading edge, bold-teal
 *  money + a tone chip + payment label on the trailing edge. The caller sizes
 *  it (per compose-modifier-and-layout-style). */
@Composable
private fun SearchResultRow(timestamp: String, o: OrderSummaryView, currency: String, modifier: Modifier = Modifier) {
    val c = madarColors()
    // A voided / failed order is dead money — mute + strike its total so the
    // hero teal reads only on live orders (mirrors the history screen).
    val dead = o.status == "voided" || o.status == "failed"
    val shape = RoundedCornerShape(Radii.md)
    Row(
        modifier.elevation(Elevation.CARD, shape)
            .clip(shape).background(c.surface)
            .border(1.dp, c.borderLight, shape)
            .padding(horizontal = Space.lg, vertical = Space.md),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            Text("#${o.orderNumber ?: "—"}", style = Type.h3(), color = c.textPrimary)
            Text(timestamp, style = Type.bodySm(), color = c.textMuted)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            // Money is the hero — bold teal; struck + muted once voided.
            Text(
                Money.format(o.totalMinor, currency),
                style = Type.money(17.sp), color = if (dead) c.textMuted else c.accent,
                textDecoration = if (dead) TextDecoration.LineThrough else null,
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                Text(o.paymentLabel, style = Type.labelSm(), color = c.textMuted)
                StatusChip(o.status, statusTone(o.status))
            }
        }
    }
}
