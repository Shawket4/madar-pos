package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.ShiftView
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// Order screen — placeholder. The shift is open; the catalog + cart land next.
// Per the design language the order screen's action bar is the only nav hub.
// Mirror of the SwiftUI OrderView.
@Composable
fun OrderScreen(model: AppModel) {
    val c = sufrixColors()

    // Reconcile with the server on appear — catches a shift force-closed from the
    // dashboard (routes back to open-shift if it's gone).
    LaunchedEffect(Unit) { model.reconcileShift() }

    Box(Modifier.fillMaxSize().background(c.bg), contentAlignment = Alignment.Center) {
        Column(
            Modifier.widthIn(max = 380.dp).padding(Space.xxl),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            SufrixMark(size = 56.dp)
            Text(t("order.title"), color = c.textPrimary, fontWeight = FontWeight.Black, fontSize = 24.sp)

            model.shift?.let { s ->
                StatusChip("${s.tellerName} · ${t("home.online")}", ChipTone.SUCCESS)
                Text(
                    s.currencyDisplay(model.session?.currencyCode ?: ""),
                    color = c.textPrimary, fontWeight = FontWeight.Bold, fontSize = 20.sp,
                )
            }

            Text(
                t("order.coming_soon"), color = c.textSecondary, fontSize = 14.sp,
                textAlign = TextAlign.Center,
            )

            SufrixButton(t("home.sign_out"), { model.signOut() }, variant = BtnVariant.GHOST, fullWidth = false)
        }
    }
}

/** "EGP 500.00" — opening cash, formatted from minor units. */
fun ShiftView.currencyDisplay(code: String): String {
    val major = openingCashMinor / 100.0
    val whole = major.toLong()
    val cents = ((major - whole) * 100).toLong()
    val padded = cents.toString().padStart(2, '0')
    return "${code.uppercase()} $whole.$padded"
}
