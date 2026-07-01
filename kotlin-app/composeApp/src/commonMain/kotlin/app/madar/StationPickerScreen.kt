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
import androidx.compose.foundation.layout.height
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
import app.madar.ui.MadarCard
import app.madar.ui.MadarIcon
import app.madar.ui.MadarMark
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Responsive
import app.madar.ui.SectionHeader
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
// Mirrors the Login / OpenShift brand-panel split — name-first hero, the station
// list on its own bordered surface card. Mirror of StationPickerView.
@Composable
fun StationPickerScreen(model: AppModel) {
    val c = madarColors()
    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= Responsive.wide
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                BrandPanel(Modifier.weight(1f).fillMaxHeight())
                Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                    StationColumn(model, showLogo = false, modifier = Modifier.verticalScroll(rememberScrollState()))
                }
            }
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                StationColumn(model, showLogo = true, modifier = Modifier.verticalScroll(rememberScrollState()))
            }
        }
    }
}

@Composable
private fun StationColumn(model: AppModel, showLogo: Boolean, modifier: Modifier = Modifier) {
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
        modifier.widthIn(max = 480.dp).fillMaxWidth()
            .padding(horizontal = Space.xxl, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        if (showLogo) MadarMark(size = 56.dp)

        // ── Hero greeting (the commissioning prompt IS the hero) ───────────────
        StationGreeting(model)

        model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }

        // ── Station list on its own bordered surface card (matches the Order /
        // OpenShift raised, hairline-bordered surfaces) ────────────────────────
        MadarCard(spacing = Space.md) {
            SectionHeader(t("setup.title"), icon = "square.stack.3d.up.fill")
            when {
                loading -> Box(Modifier.fillMaxWidth().padding(Space.xl), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.accent)
                }
                stations.isEmpty() -> Text(
                    t("setup.no_stations"), color = c.textMuted, fontFamily = LocalMadarFont.current,
                    fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(vertical = Space.md),
                )
                else -> stations.forEach { st ->
                    StationCard(st, Modifier.fillMaxWidth()) { scope.launch { model.setDeviceStation(st.id) } }
                }
            }
        }

        // ── Recessive exit ─────────────────────────────────────────────────────
        MadarButton(
            t("home.sign_out"), { model.signOut() },
            variant = BtnVariant.GHOST, icon = "rectangle.portrait.and.arrow.right",
        )
    }
}

/** The commissioning hero — accent-tinted station tile, bold black title, supporting
 *  line, and the bound branch as an info chip. Mirrors the OpenShift greeting. */
@Composable
private fun StationGreeting(model: AppModel, modifier: Modifier = Modifier) {
    val c = madarColors()
    Column(
        modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Box(Modifier.size(56.dp).clip(CircleShape).background(c.accentBg), contentAlignment = Alignment.Center) {
            MadarIcon("fork.knife", tint = c.accent, size = 28.dp)
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
}

/** One selectable station — leading tone-tile + name, default flagged with an accent
 *  StatusChip and lifted with a heavier accent border + filled tile (mirrors the
 *  Kitchen "ready" card's accent emphasis), trailing chevron. Fixed row height so
 *  every station aligns. The caller owns placement via [modifier]. */
@Composable
private fun StationCard(st: KdsStationView, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(Radii.md)
    Row(
        modifier
            .pressScale(interaction)
            .elevation(Elevation.CARD, shape)
            .clip(shape)
            .background(c.surface)
            .border(
                if (st.isDefault) 2.dp else 1.dp,
                if (st.isDefault) c.accent.copy(alpha = 0.55f) else c.borderLight,
                shape,
            )
            .clickable(interactionSource = interaction, indication = null) { onClick() }
            .height(72.dp)
            .padding(horizontal = Space.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        // Default station gets a FILLED accent tile so it reads as "this one" at a
        // glance; the rest stay tinted. Same glyph, different weight.
        Box(
            Modifier.size(44.dp).clip(RoundedCornerShape(Radii.sm))
                .background(if (st.isDefault) c.accent else c.accentBg),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon("fork.knife", tint = if (st.isDefault) c.textOnAccent else c.accent, size = IconSize.xl)
        }
        Text(
            st.name, color = c.textPrimary, fontFamily = LocalMadarFont.current,
            fontWeight = FontWeight.Bold, fontSize = 17.sp, modifier = Modifier.weight(1f),
        )
        if (st.isDefault) StatusChip(t("setup.station_default"), ChipTone.ACCENT)
        MadarIcon("chevron.forward", tint = c.textMuted, size = IconSize.md)
    }
}
