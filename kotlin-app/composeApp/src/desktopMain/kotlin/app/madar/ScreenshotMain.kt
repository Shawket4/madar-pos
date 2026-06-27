package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.ImageComposeScene
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.platform.Font
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.EmptyState
import app.madar.ui.LocalLocalize
import app.madar.ui.LocalMadarColors
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarButton
import app.madar.ui.MadarCard
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Radii
import app.madar.ui.MadarColors
import app.madar.ui.MadarDark
import app.madar.ui.MadarLight
import app.madar.ui.MadarTextField
import app.madar.ui.MetricRow
import app.madar.ui.NoticeBanner
import app.madar.ui.PinPad
import app.madar.ui.RealtimeAlertCard
import app.madar.ui.RealtimeAlertData
import app.madar.ui.SectionHeader
import app.madar.ui.SelectableChip
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.Type
import app.madar.ui.madarColors
import org.jetbrains.skia.EncodedImageFormat
import java.io.File

// Offscreen screenshot harness — renders the refreshed shared components to PNG
// with the real Cairo face, no window. Driven by the gradle `screenshots` task,
// which passes the font dir + output dir as system properties. This is the
// visual-iteration + sign-off loop for the from-the-ground-up refresh: change a
// component, regenerate, eyeball, repeat — deterministic and headless.

private fun cairo(dir: File): FontFamily = FontFamily(
    Font(File(dir, "Cairo-Regular.ttf"), FontWeight.Normal),
    Font(File(dir, "Cairo-Medium.ttf"), FontWeight.Medium),
    Font(File(dir, "Cairo-SemiBold.ttf"), FontWeight.SemiBold),
    Font(File(dir, "Cairo-Bold.ttf"), FontWeight.Bold),
    Font(File(dir, "Cairo-ExtraBold.ttf"), FontWeight.ExtraBold),
)

fun main() {
    val fontDir = File(System.getProperty("madar.fontDir", "composeApp/src/commonMain/composeResources/font"))
    val outDir = File(System.getProperty("madar.outDir", "build/screenshots")).apply { mkdirs() }
    val family = cairo(fontDir)
    renderGallery(File(outDir, "gallery-light.png"), MadarLight, family)
    renderGallery(File(outDir, "gallery-dark.png"), MadarDark, family)
    renderScreens(File(outDir, "screens-light.png"), MadarLight, family)
    renderScreens(File(outDir, "screens-dark.png"), MadarDark, family)
    println("Wrote screenshots to ${outDir.absolutePath}")
}

@OptIn(ExperimentalComposeUiApi::class)
private fun renderScreens(out: File, colors: MadarColors, family: FontFamily) {
    val density = 2f
    val widthDp = 460
    val heightDp = 1180
    val scene = ImageComposeScene(
        width = (widthDp * density).toInt(),
        height = (heightDp * density).toInt(),
        density = Density(density),
    ) {
        CompositionLocalProvider(
            LocalMadarColors provides colors,
            LocalMadarFont provides family,
            LocalLocalize provides { it },
        ) {
            MaterialTheme { Screens() }
        }
    }
    val img = scene.render()
    val data = img.encodeToData(EncodedImageFormat.PNG) ?: error("PNG encode failed")
    out.writeBytes(data.bytes)
    scene.close()
}

// A showcase of the REFRESHED screen elements (built from the real shared
// components + literal data) — settle sheet, KDS ticket with the age SLA cue, and
// a delivery card with the customer-instructions callout.
@Composable
private fun Screens() {
    val c = madarColors()
    Column(
        Modifier.fillMaxSize().background(c.bg).padding(Space.xl),
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        Text("Madar — Refreshed Screens", style = Type.h1(), color = c.textPrimary)

        // ── In-app realtime alert banner (companion to the OS notification) ──
        SectionHeader("Live alert")
        RealtimeAlertCard(
            RealtimeAlertData(1, "New delivery order · D-204", "Sara A. · EGP 132.00", "delivery.created:o1"),
            onDismiss = {},
        )

        // ── Settle ticket sheet ──────────────────────────────────────────────
        SectionHeader("Settle ticket")
        MadarCard {
            Text("Ticket #A-204", style = Type.h2(), color = c.textPrimary)
            listOf("2× Cheeseburger" to "EGP 70.00", "1× Fries" to "EGP 14.00", "1× Cola" to "EGP 11.76").forEach { (n, p) ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    Text(n, style = Type.bodySm(), color = c.textSecondary, modifier = Modifier.weight(1f))
                    Text(p, style = Type.money(), color = c.textPrimary)
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                Text("Total", style = Type.title(), color = c.textSecondary)
                Box(Modifier.weight(1f))
                Text("EGP 95.76", style = Type.moneyLg(), color = c.accent)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                SelectableChip("Cash", true, {}, icon = "banknote")
                SelectableChip("Card", false, {}, icon = "creditcard")
            }
            MadarButton("Settle", {}, icon = "checkmark.circle")
        }

        // ── KDS ticket with age SLA cue (amber border at 6m) ─────────────────
        SectionHeader("Kitchen ticket · 6m")
        val amber = c.warning
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(c.surface)
                .border(1.dp, amber.copy(alpha = 0.25f), RoundedCornerShape(14.dp)).padding(Space.md),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                Text("Table 7", style = Type.h3(), color = c.textPrimary)
                Box(Modifier.weight(1f))
                Text("6m", style = Type.money(13.sp), color = amber)
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                MadarIcon("circle", tint = c.textMuted, size = 18.dp)
                Text("2× Margherita Pizza", style = Type.body(), color = c.textPrimary, modifier = Modifier.weight(1f))
                Text("GRILL", style = Type.labelSm(), color = c.textMuted)
            }
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                MadarIcon("checkmark.circle", tint = c.success, size = 18.dp)
                Text("1× Garlic Bread", style = Type.body(), color = c.textMuted, modifier = Modifier.weight(1f))
                Text("OVEN", style = Type.labelSm(), color = c.textMuted)
            }
        }

        // ── Delivery card with customer-instructions callout ─────────────────
        SectionHeader("Delivery order")
        MadarCard {
            Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                StatusChip("Preparing", ChipTone.WARNING)
                StatusChip("In-Mall", ChipTone.NEUTRAL)
            }
            Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                Text("Sara A.", style = Type.h3(), color = c.textPrimary)
                Box(Modifier.weight(1f))
                Text("EGP 132.00", style = Type.money(15.sp), color = c.accent)
            }
            Text("+20 100 555 0192", style = Type.bodySm(), color = c.textSecondary)
            Text("Mall of Madar · Gate 3 · Shop 21", style = Type.bodySm(), color = c.textSecondary)
            Row(verticalAlignment = androidx.compose.ui.Alignment.Top, horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
                MadarIcon("text.bubble", tint = c.warning, size = IconSize.sm)
                Text("Leave at the door, call on arrival", style = Type.bodySm(), color = c.warning)
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
private fun renderGallery(out: File, colors: MadarColors, family: FontFamily) {
    val density = 2f
    val widthDp = 460
    val heightDp = 1140
    val scene = ImageComposeScene(
        width = (widthDp * density).toInt(),
        height = (heightDp * density).toInt(),
        density = Density(density),
    ) {
        CompositionLocalProvider(
            LocalMadarColors provides colors,
            LocalMadarFont provides family,
            LocalLocalize provides { it },
        ) {
            MaterialTheme { Gallery() }
        }
    }
    val img = scene.render()
    val data = img.encodeToData(EncodedImageFormat.PNG) ?: error("PNG encode failed")
    out.writeBytes(data.bytes)
    scene.close()
}

@Composable
private fun Gallery() {
    val c = madarColors()
    Column(
        Modifier.fillMaxSize().background(c.bg).padding(Space.xl),
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        Text("Madar — Component Refresh", style = Type.h1(), color = c.textPrimary)

        SectionHeader("Buttons")
        Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            Box(Modifier.weight(1f)) { MadarButton("Place order", {}, icon = "checkmark.circle.fill") }
            Box(Modifier.weight(1f)) { MadarButton("Cancel", {}, variant = BtnVariant.OUTLINE) }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            Box(Modifier.weight(1f)) { MadarButton("Void", {}, variant = BtnVariant.DANGER) }
            Box(Modifier.weight(1f)) { MadarButton("Disabled", {}, enabled = false) }
        }

        SectionHeader("Inputs")
        MadarTextField("", {}, "Manager email", icon = "envelope")
        MadarTextField("4 Madar Street", {}, "Address", icon = "mappin")

        SectionHeader("Status & filters")
        Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            StatusChip("Online", ChipTone.SUCCESS, icon = "wifi")
            StatusChip("Offline", ChipTone.WARNING, icon = "wifi.slash")
            StatusChip("3 queued", ChipTone.INFO)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            SelectableChip("Cash", true, {}, icon = "banknote")
            SelectableChip("Card", false, {}, icon = "creditcard")
        }

        NoticeBanner(
            "Working offline — changes sync when you reconnect",
            tone = ChipTone.WARNING, icon = "wifi.slash",
            actionLabel = "Sign in", onAction = {},
        )

        SectionHeader("Totals")
        MadarCard {
            MetricRow("Subtotal", "EGP 84.00")
            MetricRow("Tax", "EGP 11.76")
            Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                Text("Total", style = Type.h3(), color = c.textPrimary)
                Box(Modifier.weight(1f))
                Text("EGP 95.76", style = Type.moneyDisplay(), color = c.accent)
            }
        }

        SectionHeader("PIN entry")
        Box(Modifier.fillMaxWidth(), contentAlignment = androidx.compose.ui.Alignment.Center) {
            PinPad(pin = "34", keySize = 56.dp, onDigit = {}, onBackspace = {})
        }
    }
}
