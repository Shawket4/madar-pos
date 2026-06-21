package app.sufrix

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
import app.sufrix.ui.BtnVariant
import app.sufrix.ui.ChipTone
import app.sufrix.ui.NoticeBanner
import app.sufrix.ui.PinPad
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixLockup
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.SufrixTextField
import app.sufrix.ui.disclosureGlyph
import app.sufrix.ui.pressScale
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import app.sufrix.core.BranchView
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

// Login — branch-gated brand moment (replicates Flutter). Manager device-setup
// until the till is bound to a branch, then teller PIN with a reconfigure link.
// Wide screens (iPad / desktop) split into a brand panel + form.
@Composable
fun LoginScreen(model: AppModel) {
    val c = sufrixColors()
    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= 760.dp
        if (wide) {
            androidx.compose.foundation.layout.Row(Modifier.fillMaxSize()) {
                BrandPanel(Modifier.weight(1f).fillMaxHeight())
                Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
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
    val c = sufrixColors()
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

    Column(
        Modifier.offset { IntOffset(offsetX.value.roundToInt(), 0) },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.xl),
    ) {
        if (showLogo) SufrixMark(size = 60.dp)
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            Text(t("login.welcome_back"), color = c.textPrimary, fontWeight = FontWeight.Black, fontSize = 24.sp)
            Text(t("login.subtitle"), color = c.textSecondary, fontSize = 14.sp)
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            val branchLabel = if (model.branchName.isNotBlank()) model.branchName else "${t("login.branch")} ${model.branchId.take(8)}"
            StatusChip(branchLabel, ChipTone.INFO)
            SufrixButton(t("login.reconfigure"), { model.beginReconfigure() }, variant = BtnVariant.GHOST, fullWidth = false, height = 32.dp)
        }
        SufrixTextField(name, { name = it }, t("login.name"), enabled = !model.isBusy)
        PinPad(pin, maxPin, onDigit = ::digit, onBackspace = { if (pin.isNotEmpty()) pin = pin.dropLast(1) })
        model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
        SufrixButton(t("login.sign_in"), { submit() }, loading = model.isBusy)
        Text(t("login.pin_hint"), color = c.textMuted, fontSize = 12.sp)
    }
}

@Composable
private fun DeviceSetupForm(model: AppModel, showLogo: Boolean) {
    val c = sufrixColors()
    val scope = rememberCoroutineScope()
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    val picking = model.setupPhase == SetupPhase.PICK_BRANCH

    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
        if (showLogo) SufrixMark(size = 56.dp)
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
            Text(if (picking) t("setup.choose_branch") else t("setup.title"), color = c.textPrimary, fontWeight = FontWeight.Black, fontSize = 22.sp)
            Text(
                if (picking) t("setup.choose_branch_desc") else t("setup.desc"),
                color = c.textSecondary, fontSize = 13.sp, textAlign = TextAlign.Center,
            )
        }
        if (picking) {
            model.branches.forEach { b -> BranchRow(b) { model.bindBranch(b) } }
        } else {
            SufrixTextField(email, { email = it }, t("setup.email"), enabled = !model.isBusy, keyboard = KeyboardType.Email)
            SufrixTextField(password, { password = it }, t("setup.password"), secure = true, enabled = !model.isBusy)
        }
        model.error?.let { NoticeBanner(it, ChipTone.DANGER) }
        if (!picking) {
            SufrixButton(t("setup.continue"), { scope.launch { model.authenticateManager(email.trim(), password) } }, loading = model.isBusy)
        }
        if (picking || model.isBranchConfigured) {
            SufrixButton(t("setup.cancel"), { model.cancelReconfigure() }, variant = BtnVariant.GHOST)
        }
    }
}

@Composable
private fun BranchRow(branch: BranchView, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.sm))
            .background(c.surface).border(1.dp, c.border, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = 14.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(branch.name, color = c.textPrimary, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, modifier = Modifier.weight(1f))
        Text(disclosureGlyph(), color = c.textMuted, fontSize = 18.sp)
    }
}

