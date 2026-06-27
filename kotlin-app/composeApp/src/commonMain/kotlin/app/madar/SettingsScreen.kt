package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.MadarButton
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarTextField
import app.madar.ui.ThemeMode
import app.madar.ui.backGlyph
import app.madar.ui.disclosureGlyph
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Elevation
import app.madar.ui.elevation

// Settings — appearance, language (live en/ar), device reconfigure, diagnostics,
// sign out. Full-screen over the order screen. Mirror of the SwiftUI SettingsView.
@Composable
fun SettingsScreen(model: AppModel) {
    val c = madarColors()
    LaunchedEffect(Unit) { model.clearError() }
    Column(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                MadarIcon("chevron.backward", tint = c.textPrimary, size = 17.dp, modifier = Modifier.clickable { model.showSettings = false })
                Text(t("settings.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 17.sp)
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                model.error?.let { NoticeBanner(it, ChipTone.WARNING, icon = "exclamationmark.circle") }
                Card(t("settings.account")) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        Box(Modifier.size(48.dp).clip(RoundedCornerShape(Radii.sm)).background(c.navyBg), contentAlignment = Alignment.Center) {
                            Text((model.shift?.tellerName ?: "?").take(1).uppercase(), color = c.navy,
                                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                        }
                        Column(Modifier.weight(1f)) {
                            Text(model.shift?.tellerName ?: "—", color = c.textPrimary,
                                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                            if (model.branchName.isNotBlank()) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
                                    MadarIcon("storefront", tint = c.textMuted, size = IconSize.sm)
                                    Text(model.branchName, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
                                }
                            }
                        }
                        model.session?.role?.takeIf { it.isNotBlank() }?.let {
                            StatusChip(it.replace('_', ' ').uppercase(), ChipTone.INFO)
                        }
                    }
                }
                Card(t("settings.appearance")) {
                    Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        Chip(Modifier.weight(1f), t("settings.theme_light"), model.themeMode == ThemeMode.LIGHT) { model.setThemeMode(ThemeMode.LIGHT) }
                        Chip(Modifier.weight(1f), t("settings.theme_dark"), model.themeMode == ThemeMode.DARK) { model.setThemeMode(ThemeMode.DARK) }
                        Chip(Modifier.weight(1f), t("settings.theme_system"), model.themeMode == ThemeMode.SYSTEM) { model.setThemeMode(ThemeMode.SYSTEM) }
                    }
                }
                Card(t("settings.language")) {
                    Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        Chip(Modifier.weight(1f), "English", model.locale.startsWith("en")) { model.setLocale("en") }
                        Chip(Modifier.weight(1f), "العربية", model.locale.startsWith("ar")) { model.setLocale("ar") }
                    }
                }
                Card(t("settings.printer")) {
                    // This till's code (the <DEVICE> segment of every order_ref) lives
                    // in the printer card alongside the printer host + brand (matches Swift).
                    MadarTextField(model.deviceCode, { model.setDeviceCode(it) }, t("settings.device_code_hint"), icon = "number")
                    Text(t("settings.device_code_caption"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp)
                    MadarTextField(model.printerHost, { model.setPrinterHost(it) }, t("settings.printer_hint"), icon = "printer")
                    Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        Chip(Modifier.weight(1f), t("settings.printer_epson"),
                            model.printerBrand == app.madar.core.PrinterBrand.EPSON) {
                            model.setPrinterBrand(app.madar.core.PrinterBrand.EPSON)
                        }
                        Chip(Modifier.weight(1f), t("settings.printer_star"),
                            model.printerBrand == app.madar.core.PrinterBrand.STAR) {
                            model.setPrinterBrand(app.madar.core.PrinterBrand.STAR)
                        }
                    }
                }
                Card(t("settings.lan")) {
                    // Optional fixed hub-IP for the LAN relay when mDNS auto-discovery
                    // can't reach peers. Writes route through the core (setLanHub),
                    // which registers it live if the relay is running and clears on blank.
                    MadarTextField(model.lanHub, { model.setLanHub(it) }, t("settings.lan_hub_hint"), icon = "wifi")
                    Text(t("settings.lan_caption"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 11.sp)
                    InfoRow(
                        if (model.lanRelayActive) t("settings.lan_active") else t("settings.lan_offline"),
                        if (model.lanRelayActive) "${model.lanPeerCount} ${t("settings.lan_peers")}" else "—",
                    )
                }
                Card(t("settings.device")) {
                    Row(
                        Modifier.fillMaxWidth().clickable {
                            // Re-provisioning is only allowed with a closed drawer.
                            if (model.hasOpenShift) {
                                model.flagError(model.t("settings.reconfigure_shift_open"))
                            } else {
                                model.beginReconfigure(); model.showSettings = false
                            }
                        },
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(Space.lg),
                    ) {
                        MadarIcon("building.2", tint = c.textSecondary, size = 20.dp)
                        Text(t("settings.reconfigure"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                        Box(Modifier.weight(1f))
                        MadarIcon("chevron.right", tint = c.textMuted, size = IconSize.md)
                    }
                }
                LaunchedEffect(Unit) { model.loadDiagnostics() }
                Card(t("settings.diagnostics")) {
                    InfoRow(t("settings.version"), model.core.version())
                    InfoRow(t("settings.server"), model.core.baseUrl())
                    InfoRow(t("settings.pending"), "${model.pendingCount}")
                    if (model.diagnostics.isNotEmpty()) {
                        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Text(t("settings.recent_warnings"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                            Box(Modifier.weight(1f))
                            Text(t("settings.clear"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                                modifier = Modifier.clickable { model.clearDiagnostics() })
                        }
                        model.diagnostics.take(15).forEach { e ->
                            Column(Modifier.fillMaxWidth()) {
                                Text(e.message, color = if (e.level == "error") c.danger else c.warning, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
                                Text(e.at, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 10.sp)
                            }
                        }
                    }
                }
                MadarButton(
                    t("settings.sign_out"),
                    {
                        // Sign-out (→ login) requires a closed drawer first.
                        if (model.hasOpenShift) {
                            model.flagError(model.t("settings.sign_out_shift_open"))
                        } else {
                            model.signOut(); model.showSettings = false
                        }
                    },
                    variant = BtnVariant.DANGER,
                    icon = "rectangle.portrait.and.arrow.right",
                )
            }
        }
    }
}

@Composable
private fun Card(title: String, content: @Composable () -> Unit) {
    val c = madarColors()
    Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        Text(title.uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp, letterSpacing = 0.6.sp)
        Column(
            Modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.md)).clip(RoundedCornerShape(Radii.md)).background(c.surface)
                .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) { content() }
    }
}

@Composable
private fun Chip(modifier: Modifier, label: String, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
    Box(
        modifier.clip(RoundedCornerShape(Radii.sm)).background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable { onClick() }.padding(vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    val c = madarColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.sp)
        Box(Modifier.weight(1f).padding(start = Space.md))
        Text(value, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}
