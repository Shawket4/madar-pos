package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.ThemeMode
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// Settings — appearance, language (live en/ar), device reconfigure, diagnostics,
// sign out. Full-screen over the order screen. Mirror of the SwiftUI SettingsView.
@Composable
fun SettingsScreen(model: AppModel) {
    val c = sufrixColors()
    Column(Modifier.fillMaxSize().background(c.bg)) {
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Text("‹", color = c.textPrimary, fontSize = 26.sp, modifier = Modifier.clickable { model.showSettings = false })
                Text(t("settings.title"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 17.sp)
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(Modifier.widthIn(max = 480.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
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
                    SufrixTextField(model.printerHost, { model.setPrinterHost(it) }, t("settings.printer_hint"))
                }
                Card(t("settings.device")) {
                    Row(
                        Modifier.fillMaxWidth().clickable { model.beginReconfigure(); model.showSettings = false },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(t("settings.reconfigure"), color = c.textPrimary, fontFamily = SufrixFont, fontSize = 14.sp)
                        Box(Modifier.weight(1f))
                        Text("›", color = c.textMuted, fontSize = 18.sp)
                    }
                }
                Card(t("settings.diagnostics")) {
                    InfoRow(t("settings.version"), model.core.version())
                    InfoRow(t("settings.server"), model.core.baseUrl())
                    InfoRow(t("settings.pending"), "${model.pendingCount}")
                }
                SufrixButton(t("settings.sign_out"), { model.signOut(); model.showSettings = false }, variant = BtnVariant.DANGER)
            }
        }
    }
}

@Composable
private fun Card(title: String, content: @Composable () -> Unit) {
    val c = sufrixColors()
    Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        Text(title.uppercase(), color = c.textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
                .border(1.dp, c.border, RoundedCornerShape(Radii.md)).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) { content() }
    }
}

@Composable
private fun Chip(modifier: Modifier, label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    Box(
        modifier.clip(RoundedCornerShape(Radii.sm)).background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable { onClick() }.padding(vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 13.sp)
        Box(Modifier.weight(1f).padding(start = Space.md))
        Text(value, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}
