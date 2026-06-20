package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.ui.AmountField
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import kotlinx.coroutines.launch

// Open-shift screen — the gate between sign-in and selling. Count the drawer's
// opening cash and open the shift (writes locally + queues; works offline).
// Mirror of the SwiftUI OpenShiftView.
@Composable
fun OpenShiftScreen(model: AppModel) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var openingMinor by remember { mutableStateOf(0L) }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState())
                .padding(horizontal = Space.xxl, vertical = 48.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = 400.dp).fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(Space.xl),
            ) {
                SufrixMark(size = 56.dp)

                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(Space.xs),
                ) {
                    Text(t("shift.open_title"), color = c.textPrimary, fontWeight = FontWeight.Black, fontSize = 24.sp)
                    Text(
                        t("shift.opening_desc"), color = c.textSecondary, fontSize = 14.sp,
                        textAlign = TextAlign.Center,
                    )
                }

                model.session?.let { s ->
                    StatusChip("${t("shift.signed_in_as")} ${s.displayName}", ChipTone.INFO)
                }

                Column(
                    Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    Text(t("shift.opening_cash"), color = c.textMuted, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                    AmountField(
                        amountMinor = openingMinor,
                        onAmountMinor = { openingMinor = it },
                        currencyCode = model.session?.currencyCode ?: "",
                    )
                }

                model.error?.let { NoticeBanner(it, ChipTone.DANGER) }

                SufrixButton(t("shift.open_button"), { scope.launch { model.openShift(openingMinor) } }, loading = model.isBusy)
                SufrixButton(t("shift.switch_teller"), { model.signOut() }, variant = BtnVariant.GHOST, fullWidth = false, height = 32.dp)
            }
        }
    }
}
