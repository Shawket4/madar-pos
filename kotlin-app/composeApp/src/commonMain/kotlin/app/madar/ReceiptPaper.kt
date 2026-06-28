package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.ReceiptLineView
import app.madar.core.ReceiptModifierView
import app.madar.core.ReceiptView
import app.madar.ui.BtnVariant
import app.madar.ui.Money
import app.madar.ui.Space
import app.madar.ui.MadarButton
import app.madar.ui.LocalMadarFont
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import coil3.compose.AsyncImage
import kotlinx.coroutines.launch

// On-screen receipt preview — a white "thermal paper" card rendered from the
// core's ReceiptView so the teller sees exactly what will print BEFORE sending
// it. Mirrors the Swift ReceiptPaper + the ESC/POS layout in receipt.rs.
// Theme-invariant: a receipt is always white paper with dark ink.

private val Paper = Color(0xFFFFFFFF)
private val Ink = Color(0xFF1A1A1A)
private val Faint = Color(0xFF6B6B6B)
private val Rule = Color(0xFFCCCCCC)

@Composable
fun ReceiptPaper(
    model: AppModel,
    receipt: ReceiptView,
    storeName: String,
    currency: String,
    orgLogoUrl: String? = null,
    modifier: Modifier = Modifier,
) {
    fun money(m: Long) = Money.format(m, currency)
    Column(
        modifier
            .widthIn(max = 360.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(Paper)
            .border(1.dp, Rule, RoundedCornerShape(10.dp))
            .padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
            // Org brand mark at the top of the paper (mirrors Flutter's
            // _buildLogoAndBranch / Swift's logo header). Blank/unreachable →
            // just the store name; Coil draws nothing while loading or on failure.
            if (!orgLogoUrl.isNullOrBlank()) {
                // Aspect-preserved (Fit) within a natural box — a wide wordmark or a
                // square mark both render without cropping or squishing, matching
                // Flutter / the Swift receipt. NOT a fixed 64x64 square.
                AsyncImage(
                    model = orgLogoUrl,
                    contentDescription = null,
                    modifier = Modifier.heightIn(max = 60.dp).widthIn(max = 220.dp).padding(bottom = 6.dp),
                    contentScale = ContentScale.Fit,
                )
            }
            if (receipt.isVoided) mono("*** ${t("receipt.voided")} ***", 13, FontWeight.Bold, Color(0xFFB71C1C))
            mono(if (storeName.isBlank()) "MADAR" else storeName.uppercase(), 15, FontWeight.Bold, Ink)
            if (receipt.isDelivery && receipt.deliveryChannel != null) {
                val ch = if (receipt.deliveryChannel == "in_mall") t("delivery.in_mall") else t("receipt.delivery")
                mono("— ${ch.uppercase()} —", 11, FontWeight.Normal, Faint)
            }
        }
        rule()
        moneyRow(orderTitle(receipt, t("receipt.order")), model.fmtReceipt(receipt.createdAt))
        receipt.orderRef?.let { moneyRow("${t("receipt.ref")}: $it", "") }
        rule()
        if (receipt.isDelivery) {
            receipt.customerName?.let { moneyRow(t("receipt.customer"), it) }
            receipt.customerPhone?.let { moneyRow(t("receipt.phone"), it) }
            receipt.deliveryAddress?.let { mono("${t("receipt.address")} $it", 12, FontWeight.Normal, Ink) }
            receipt.deliveryZone?.let { moneyRow(t("receipt.zone"), it) }
            // Courier ref + COD/payment hint + customer instructions (matches the
            // printed raster receipt; were dropped from the preview).
            receipt.deliveryRef?.let { moneyRow(t("receipt.delivery_ref"), it) }
            receipt.paymentHint?.let { moneyRow(t("receipt.payment_hint"), it) }
            receipt.deliveryNotes?.takeIf { it.isNotBlank() }?.let { mono("${t("receipt.notes")} $it", 12, FontWeight.Normal, Ink) }
            rule()
        }
        receipt.lines.forEach { line -> lineBlock(line, ::money) }
        rule()
        moneyRow(t("order.subtotal"), money(receipt.subtotalMinor))
        if (receipt.discountMinor > 0) moneyRow(t("order.discount"), "−${money(receipt.discountMinor)}")
        if (receipt.taxMinor > 0) moneyRow(t("order.tax"), money(receipt.taxMinor))
        if (receipt.deliveryFeeMinor > 0) moneyRow(t("receipt.delivery_fee"), money(receipt.deliveryFeeMinor))
        moneyRow(t("order.total").uppercase(), money(receipt.totalMinor), bold = true)
        if (receipt.tipMinor > 0) moneyRow(t("order.tip"), money(receipt.tipMinor))
        if (receipt.isCash) {
            moneyRow(t("receipt.cash"), money(receipt.amountTenderedMinor))
            moneyRow(t("order.change"), money(receipt.changeMinor))
        }
        rule()
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
            mono(receipt.paymentLabel.uppercase(), 11, FontWeight.SemiBold, Ink)
            receipt.tellerName?.let { mono("${t("receipt.served_by")} $it", 11, FontWeight.Normal, Faint) }
            mono(t("receipt.thank_you"), 12, FontWeight.Normal, Ink)
        }
    }
}

private fun orderTitle(r: ReceiptView, orderLabel: String): String =
    r.orderNumber?.let { "$orderLabel #$it" }
        ?: "$orderLabel ${(r.localOrderId.substringBefore('-')).uppercase()}"

@Composable
private fun mono(text: String, size: Int, weight: FontWeight, color: Color) {
    Text(text, color = color, fontFamily = LocalMadarFont.current, fontWeight = weight, fontSize = size.sp)
}

@Composable
private fun rule() {
    Box(Modifier.fillMaxWidth().padding(vertical = 1.dp).height(1.dp).background(Rule))
}

@Composable
private fun moneyRow(left: String, right: String, bold: Boolean = false, faint: Boolean = false) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
        Text(
            left, color = if (faint) Faint else Ink, fontFamily = LocalMadarFont.current,
            fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal,
            fontSize = (if (bold) 13 else 12).sp, modifier = Modifier.weight(1f),
        )
        if (right.isNotEmpty()) {
            Text(
                right, color = if (faint) Faint else Ink, fontFamily = LocalMadarFont.current,
                fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal,
                fontSize = (if (bold) 13 else 12).sp,
            )
        }
    }
}

@Composable
private fun lineBlock(line: ReceiptLineView, money: (Long) -> String) {
    moneyRow("${line.qty}× ${nameWithSize(line.name, line.sizeLabel)}", money(line.lineTotalMinor))
    if (line.isBundle) {
        line.components.forEach { c ->
            mono("  – ${nameWithSize(c.name, c.sizeLabel)}", 12, FontWeight.Normal, Faint)
            c.addons.forEach { modRow("    + ", it, money) }
            c.optionals.forEach { modRow("    + ", it, money) }
        }
    } else {
        line.addons.forEach { modRow("  + ", it, money) }
        line.optionals.forEach { modRow("  + ", it, money) }
    }
}

@Composable
private fun modRow(prefix: String, m: ReceiptModifierView, money: (Long) -> String) {
    moneyRow("$prefix${m.name}", if (m.priceMinor > 0) "+${money(m.priceMinor)}" else "", faint = true)
}

private fun nameWithSize(base: String, size: String?): String =
    if (!size.isNullOrEmpty()) "$base ($size)" else base

/** Full-screen preview of a past order's receipt with a Print action (#3). */
@Composable
fun ReceiptPreviewScreen(model: AppModel, receipt: ReceiptView, onClose: () -> Unit) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    var printing by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(false) }
    Column(Modifier.fillMaxSize().background(c.bg)) {
        Row(
            Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t("receipt.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 18.sp, modifier = Modifier.weight(1f))
            MadarIcon("xmark", tint = c.textMuted, size = IconSize.md,
                modifier = Modifier.clip(CircleShape).clickable { onClose() }.padding(8.dp))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            ReceiptPaper(model, receipt, model.branchName, currency, model.orgLogoUrl)
        }
        Column(Modifier.fillMaxWidth().background(c.surface).padding(Space.lg)) {
            MadarButton(
                label = t("receipt.print"),
                icon = "printer",
                onClick = { printing = true; scope.launch { model.printReceiptView(receipt); printing = false } },
                loading = printing,
            )
        }
    }
}
