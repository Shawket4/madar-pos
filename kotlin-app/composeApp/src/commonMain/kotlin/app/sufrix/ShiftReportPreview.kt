package app.sufrix

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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.ShiftReportPaymentLine
import app.sufrix.core.ShiftReportView
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.launch

// The Z-report breakdown (per-method sales with proportional bars + drawer
// movements + totals), reused mid-shift (a print sheet) and in close-shift.
// Mirrors the Swift ShiftReportBreakdown / ShiftReportPreviewView.

@Composable
fun ShiftReportBreakdown(report: ShiftReportView, currency: String, modifier: Modifier = Modifier) {
    val c = sufrixColors()
    fun money(m: Long) = Money.format(m, currency)
    val maxLine = (report.paymentLines.maxOfOrNull { it.totalMinor } ?: 1L).coerceAtLeast(1L)
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.md)) {
        if (report.paymentLines.isEmpty()) {
            Text(t("history.empty"), color = c.textMuted, fontFamily = SufrixFont, fontSize = 12.sp)
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                report.paymentLines.forEach { p -> methodRow(p, maxLine, ::money) }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        if (report.cashInMinor > 0) totalRow(t("shift.cash_in"), money(report.cashInMinor), c.success)
        if (report.cashOutMinor > 0) totalRow(t("shift.cash_out"), "−${money(report.cashOutMinor)}", c.danger)
        if (report.cashMovements.isNotEmpty()) {
            Column(Modifier.padding(start = Space.sm), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                report.cashMovements.forEach { m ->
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            m.note.ifBlank { m.movedByName }, color = c.textMuted, fontFamily = SufrixFont,
                            fontSize = 11.sp, maxLines = 1, modifier = Modifier.weight(1f),
                        )
                        Text(
                            (if (m.amountMinor < 0) "−" else "+") + Money.format(kotlin.math.abs(m.amountMinor), currency),
                            color = if (m.amountMinor < 0) c.danger else c.success,
                            fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 11.sp,
                        )
                    }
                }
            }
        }
        if (report.voidedAmountMinor > 0) totalRow(t("history.voided"), "−${money(report.voidedAmountMinor)}", c.danger)
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        totalRow(t("shift.payments"), money(report.totalPaymentsMinor), c.textPrimary)
        totalRow(t("shift.expected_cash"), money(report.expectedCashMinor), c.textPrimary, emphasized = true)
    }
}

@Composable
private fun methodRow(p: ShiftReportPaymentLine, maxLine: Long, money: (Long) -> String) {
    val c = sufrixColors()
    val frac = (p.totalMinor.toFloat() / maxLine.toFloat()).coerceIn(0.02f, 1f)
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(p.method, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            Text(" · ${p.orderCount}", color = c.textMuted, fontFamily = SufrixFont, fontSize = 11.sp)
            Spacer(Modifier.weight(1f))
            Text(money(p.totalMinor), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        Box(Modifier.fillMaxWidth().height(5.dp).clip(CircleShape).background(c.surfaceAlt)) {
            Box(Modifier.fillMaxWidth(frac).height(5.dp).clip(CircleShape).background(if (p.isCash) c.success else c.accent))
        }
    }
}

@Composable
private fun totalRow(label: String, value: String, tone: Color, emphasized: Boolean = false) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium, fontSize = (if (emphasized) 15 else 13).sp,
            modifier = Modifier.weight(1f),
        )
        Text(value, color = tone, fontFamily = SufrixFont, fontWeight = if (emphasized) FontWeight.Black else FontWeight.SemiBold, fontSize = (if (emphasized) 16 else 13).sp)
    }
}

/** Mid-shift report preview — full-screen, Print without closing the shift. */
@Composable
fun ShiftReportPreviewScreen(model: AppModel, onClose: () -> Unit) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    LaunchedEffect(Unit) { model.loadShiftReport() }
    Column(Modifier.fillMaxSize().background(c.bg)) {
        // header
        Row(
            Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("‹", color = c.textPrimary, fontFamily = SufrixFont, fontSize = 22.sp,
                modifier = Modifier.clip(CircleShape).clickable { onClose() }.padding(horizontal = 6.dp))
            Spacer(Modifier.width8())
            Column(Modifier.weight(1f)) {
                Text(t("shift.report_title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 18.sp)
                model.shift?.let { Text(it.tellerName, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp) }
            }
            model.shiftReport?.let {
                StatusChip(if (it.fromServer) t("chrome.online") else t("chrome.offline"),
                    if (it.fromServer) ChipTone.SUCCESS else ChipTone.WARNING)
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        // body
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(Space.lg)) {
            Box(Modifier.fillMaxWidth().widthIn(max = 460.dp)) {
                model.shiftReport?.let { ShiftReportBreakdown(it, currency) }
            }
        }
        // footer
        Column(Modifier.fillMaxWidth().background(c.surface).padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            SufrixButton(
                label = if (model.printState == PrintState.PRINTING) t("receipt.printing") else t("shift.print_report"),
                onClick = { scope.launch { model.printShiftReport() } },
                loading = model.printState == PrintState.PRINTING,
            )
            SufrixButton(label = t("common.done"), onClick = onClose, variant = BtnVariant.OUTLINE)
        }
    }
}

private fun Modifier.width8() = this.then(androidx.compose.foundation.layout.width(8.dp))
