package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.LocalLocalize
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.SufrixTheme
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// Shared Compose host (Android + desktop). Thin: renders what the core hands it
// and routes Login ↔ home at deliberate boundaries only (PLAN §R11). Default
// theme is light; localization + RTL come from the core.
@Composable
fun App(model: AppModel) {
    SufrixTheme(mode = model.themeMode) {
        CompositionLocalProvider(
            LocalLocalize provides { key -> model.t(key) },
            LocalLayoutDirection provides if (model.isRTL) LayoutDirection.Rtl else LayoutDirection.Ltr,
        ) {
            Box(Modifier.fillMaxSize().background(sufrixColors().bg)) {
                if (model.isSignedIn) HomeScreen(model) else LoginScreen(model)
            }
        }
    }
}

/** Convenience for entry points that hold the core + vault but not yet a model. */
@Composable
fun App(core: app.sufrix.core.SufrixCore, vault: HostVault) {
    val model = remember { AppModel(core, vault) }
    App(model)
}

@Composable
private fun HomeScreen(model: AppModel) {
    val c = sufrixColors()
    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp).widthIn(max = 360.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        SufrixMark(size = 56.dp)
        Spacer(Modifier.height(Space.lg))
        Text(t("home.signed_in"), color = c.textPrimary, fontWeight = FontWeight.Black, fontSize = 24.sp)
        Spacer(Modifier.height(Space.md))
        model.session?.let { s ->
            StatusChip(if (s.online) t("home.online") else t("home.offline"), if (s.online) ChipTone.SUCCESS else ChipTone.WARNING)
            Spacer(Modifier.height(Space.md))
            StatRow(t("home.teller"), s.displayName)
            StatRow(t("home.role"), s.role)
            StatRow(t("home.currency"), s.currencyCode)
        }
        Spacer(Modifier.height(Space.lg))
        SufrixButton(t("home.sign_out"), { model.signOut() }, variant = BtnVariant.DANGER, fullWidth = false)
    }
}

@Composable
private fun StatRow(label: String, value: String) {
    val c = sufrixColors()
    Row(Modifier.padding(vertical = 2.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, color = c.textSecondary, fontSize = 13.sp)
        Text(value, color = c.textPrimary, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
    }
}
