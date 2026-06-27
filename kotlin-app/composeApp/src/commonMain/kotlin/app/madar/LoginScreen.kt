package app.madar

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.LocalMadarFont
import app.madar.ui.NoticeBanner
import app.madar.ui.PinPad
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.MadarButton
import app.madar.ui.MadarLockup
import app.madar.ui.MadarMark
import app.madar.ui.MadarTextField
import app.madar.ui.disclosureGlyph
import app.madar.ui.pressScale
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Responsive
import app.madar.core.BranchView
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.input.KeyboardType
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import app.madar.ui.Elevation
import app.madar.ui.elevation

// Login — branch-gated brand moment (replicates Flutter). Manager device-setup
// until the till is bound to a branch, then teller PIN with a reconfigure link.
// Wide screens (iPad / desktop) split into a brand panel + form.
@Composable
fun LoginScreen(model: AppModel) {
    val c = madarColors()
    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= Responsive.wide
        if (wide) {
            // Flutter splits the wide layout 55/45 (brand panel : form).
            androidx.compose.foundation.layout.Row(Modifier.fillMaxSize()) {
                BrandPanel(Modifier.weight(0.55f).fillMaxHeight())
                Box(Modifier.weight(0.45f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                    FormColumn(model, showLogo = false)
                }
            }
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                FormColumn(model, showLogo = true)
            }
        }
    }
}

@Composable
private fun FormColumn(model: AppModel, showLogo: Boolean) {
    Column(
        Modifier.widthIn(max = 400.dp).fillMaxWidth().verticalScroll(rememberScrollState())
            .padding(horizontal = Space.xxl, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (model.isBranchConfigured && !model.reconfiguring) {
            TellerForm(model, showLogo)
        } else {
            DeviceSetupForm(model, showLogo)
        }
    }
}

@Composable
private fun TellerForm(model: AppModel, showLogo: Boolean) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var shake by remember { mutableStateOf(0) }
    val maxPin = 6

    val offsetX = remember { Animatable(0f) }
    LaunchedEffect(shake) {
        if (shake > 0) listOf(-8f, 8f, -6f, 6f, 0f).forEach { offsetX.animateTo(it, tween(60)) }
    }

    fun fail() { shake++ }
    fun submit() {
        if (name.isBlank() || pin.length < 4) { fail(); return }
        scope.launch {
            model.signInTeller(name.trim(), pin)
            if (model.error != null) { pin = ""; fail() }
        }
    }
    fun digit(d: String) {
        if (model.isBusy || pin.length >= maxPin) return
        model.error = null
        pin += d
        if (pin.length == maxPin) submit()
    }

    // Spacing mirrors Flutter `_buildForm`'s deliberate rhythm (not a flat
    // stack): xs between title/subtitle, sm before the branch chip block, xxl
    // after the header block, xl around the PIN pad, sm between button and hint.
    Column(
        Modifier.offset { IntOffset(offsetX.value.roundToInt(), 0) },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (showLogo) {
            MadarMark(size = 60.dp)
            Spacer(Modifier.height(Space.xxl))
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            Text(t("login.welcome_back"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 24.sp)
            Text(t("login.subtitle"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 14.sp)
        }
        Spacer(Modifier.height(Space.sm))
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            val branchLabel = if (model.branchName.isNotBlank()) model.branchName else "${t("login.branch")} ${model.branchId.take(8)}"
            StatusChip(branchLabel, ChipTone.INFO, icon = "building.2")
            Text(
                t("login.reconfigure"),
                color = c.textMuted,
                fontFamily = LocalMadarFont.current,
                fontSize = 12.sp,
                modifier = Modifier
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { model.beginReconfigure() }
                    .padding(vertical = Space.xs),
            )
        }
        Spacer(Modifier.height(Space.xxl))
        MadarTextField(name, { name = it }, t("login.name"), enabled = !model.isBusy, icon = "person")
        Spacer(Modifier.height(Space.xl))
        PinPad(pin, maxPin, onDigit = ::digit, onBackspace = { if (pin.isNotEmpty()) pin = pin.dropLast(1) })
        model.error?.let {
            Spacer(Modifier.height(Space.sm))
            NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle")
        }
        Spacer(Modifier.height(Space.xl))
        MadarButton(t("login.sign_in"), { submit() }, loading = model.isBusy, height = 52.dp)
        Spacer(Modifier.height(Space.sm))
        Text(t("login.pin_hint"), color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp, textAlign = TextAlign.Center)
    }
}

@Composable
private fun DeviceSetupForm(model: AppModel, showLogo: Boolean) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    val picking = model.setupPhase == SetupPhase.PICK_BRANCH

    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
        if (showLogo) MadarMark(size = 56.dp)
        Column(
            Modifier.padding(bottom = Space.sm),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.xs),
        ) {
            Text(if (picking) t("setup.choose_branch") else t("setup.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 22.sp)
            Text(
                if (picking) t("setup.choose_branch_desc") else t("setup.desc"),
                color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 13.5.sp, textAlign = TextAlign.Center,
            )
        }
        if (picking) {
            model.branches.forEach { b -> BranchRow(b) { model.bindBranch(b) } }
        } else {
            MadarTextField(email, { email = it }, t("setup.email"), enabled = !model.isBusy, keyboard = KeyboardType.Email, icon = "envelope")
            MadarTextField(password, { password = it }, t("setup.password"), secure = true, enabled = !model.isBusy, icon = "lock")
        }
        model.error?.let { NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle") }
        if (!picking) {
            MadarButton(t("setup.continue"), { scope.launch { model.authenticateManager(email.trim(), password) } }, loading = model.isBusy)
        }
        if (picking || model.isBranchConfigured) {
            MadarButton(t("setup.cancel"), { model.cancelReconfigure() }, variant = BtnVariant.GHOST)
        }
    }
}

@Composable
private fun BranchRow(branch: BranchView, onClick: () -> Unit) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.fillMaxWidth().pressScale(interaction)
            .elevation(Elevation.CARD, RoundedCornerShape(Radii.sm)).clip(RoundedCornerShape(Radii.sm))
            .background(c.surface).border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = 14.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        MadarIcon("building.2", tint = c.textMuted, size = IconSize.sm)
        Text(branch.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, modifier = Modifier.weight(1f))
        MadarIcon("chevron.right", tint = c.textMuted, size = IconSize.xs)
    }
}

