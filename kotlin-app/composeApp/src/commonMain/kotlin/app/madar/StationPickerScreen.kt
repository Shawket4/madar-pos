package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.KdsStationView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.Elevation
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarButton
import app.madar.ui.MadarIcon
import app.madar.ui.MadarMark
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Responsive
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.elevation
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.t
import kotlinx.coroutines.launch

// Kitchen-display commissioning — the screen a `kitchen`-role device shows once
// it's bound to a branch but has no station yet (the core routes here via
// DeviceSetup; without it a KDS device dead-ended). Pick a station → the core
// pins it (set_device_station) → the route recomputes to the KitchenDisplay.
// Mirrors the Login / OpenShift brand-panel split. Mirror of StationPickerView.
@Composable
fun StationPickerScreen(model: AppModel) {
    val c = madarColors()
    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= Responsive.wide
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                BrandPanel(Modifier.weight(1f).fillMaxHeight())
                Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                    StationColumn(model, showLogo = false)
                }
            }
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                StationColumn(model, showLogo = true)
            }
        }
    }
}

@Composable
private fun StationColumn(model: AppModel, showLogo: Boolean) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var loading by remember { mutableStateOf(true) }
    LaunchedEffect(Unit) {
        model.clearError()
        model.loadKdsStations()
        loading = false
    }
    val stations = model.kdsStations

    Column(
        Modifier.widthIn(max = 480.dp).fillMaxWidth().verticalScroll(rememberScrollState())
            .padding(horizontal = Space.xxl, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        if (showLogo) MadarMark(size = 56.dp)

        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Box(Modifier.size(56.dp).clip(CircleShape).background(c.accentBg), contentAlignment = Alignment.Center) {
                MadarIcon("flame.fill", tint = c.accent, size = 28.dp)
            }
            Text(
                t("setup.choose_station"), color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Black, fontSize = 26.sp, textAlign = TextAlign.Center,
            )
            Text(
                t("setup.choose_station_desc"), color = c.textMuted, fontFamily = LocalMadarFont.current,
                fontSize = 13.sp, textAlign = TextAlign.Center,
            )
            if (model.branchName.isNotBlank()) {
                Box(Modifier.padding(top = Space.xs)) { StatusChip(model.branchName, ChipTone.INFO, icon = "building.2") }
            }
        }

        model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }

        when {
            loading -> CircularProgressIndicator(color = c.accent, modifier = Modifier.padding(Space.xl))
            stations.isEmpty() -> Text(
                t("setup.no_stations"), color = c.textMuted, fontFamily = LocalMadarFont.current,
                fontSize = 13.sp, textAlign = TextAlign.Center,
            )
            else -> Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                stations.forEach { st -> StationCard(st) { scope.launch { model.setDeviceStation(st.id) } } }
            }
        }

        MadarButton(
            t("home.sign_out"), { model.signOut() },
            modifier = Modifier.padding(top = Space.sm), variant = BtnVariant.GHOST, icon = "rectangle.portrait.and.arrow.right",
        )
    }
}

@Composable
private fun StationCard(st: KdsStationView, onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(Radii.md)
    Row(
        Modifier.fillMaxWidth().pressScale(interaction).elevation(Elevation.CARD, shape).clip(shape)
            .background(c.surface).border(1.dp, c.borderLight, shape)
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .padding(Space.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg), contentAlignment = Alignment.Center) {
            MadarIcon("flame.fill", tint = c.accent, size = IconSize.lg)
        }
        Column(Modifier.weight(1f)) {
            Text(st.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 16.sp)
            if (st.isDefault) {
                Text(t("setup.station_default"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
            }
        }
        MadarIcon("chevron.forward", tint = c.textMuted, size = IconSize.md)
    }
}
