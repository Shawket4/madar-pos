package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.DraftView
import app.madar.ui.Elevation
import app.madar.ui.IconSize
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarIcon
import app.madar.ui.Money
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.Type
import app.madar.ui.elevation
import app.madar.ui.madarColors
import app.madar.ui.pressScale
import app.madar.ui.t

// Held orders (drafts) — parked carts the teller can restore later. Reached from
// the side nav rail / "More" drawer. Tapping a draft restores it into the cart
// (replacing the current one) and closes the screen; the trash button discards it.
// All state + rules live in the core (cart::hold/restore_draft). Full-screen over
// the order screen. Mirror of the SwiftUI DraftsView.
@Composable
fun DraftsScreen(model: AppModel) {
    val c = madarColors()
    val currency = model.session?.currencyCode ?: ""
    LaunchedEffect(Unit) { model.loadDrafts() }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        DraftsHeader(onClose = { model.showDrafts = false })

        if (model.drafts.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(Space.md),
                ) {
                    MadarIcon("tray", tint = c.textMuted, size = 36.dp)
                    Text(
                        t("drafts.empty"), color = c.textSecondary,
                        fontFamily = LocalMadarFont.current, fontSize = 14.sp,
                    )
                }
            }
        } else {
            LazyColumn(
                Modifier.fillMaxSize(),
                contentPadding = PaddingValues(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.md),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                items(model.drafts, key = { it.id }) { d ->
                    DraftCard(
                        d, currency,
                        onRestore = { model.restoreDraft(d.id); model.showDrafts = false },
                        onDiscard = { model.discardDraft(d.id) },
                        modifier = Modifier.widthIn(max = 560.dp).fillMaxWidth(),
                    )
                }
            }
        }
    }
}

/** Confident board header — back chevron, a leading teal tone-tile behind the tray
 *  glyph, and the bold title on a surface bar with a hairline (mirrors Kitchen). */
@Composable
private fun DraftsHeader(onClose: () -> Unit, modifier: Modifier = Modifier) {
    val c = madarColors()
    Column(modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            MadarIcon(
                "chevron.backward", tint = c.textPrimary, size = 17.dp,
                modifier = Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() }, indication = null,
                ) { onClose() },
            )
            Box(
                Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
                contentAlignment = Alignment.Center,
            ) {
                MadarIcon("tray.full", tint = c.accent, size = IconSize.lg)
            }
            Text(
                t("drafts.title"), color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Black, fontSize = 20.sp,
            )
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

/** A parked-cart card — leading teal tray tile, name + item count, bold teal money
 *  (the hero figure), and a danger discard tile. The whole card restores the draft. */
@Composable
private fun DraftCard(
    d: DraftView,
    currency: String,
    onRestore: () -> Unit,
    onDiscard: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Row(
        modifier
            .pressScale(interaction, 0.99f)
            .elevation(Elevation.CARD, RoundedCornerShape(Radii.md))
            .clip(RoundedCornerShape(Radii.md))
            .background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onRestore()
            }
            .padding(horizontal = Space.md, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(
            Modifier.size(44.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon("tray.full", tint = c.accent, size = IconSize.xl)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                d.name, color = c.textPrimary, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.Bold, fontSize = 16.sp,
            )
            Text(
                "${d.itemCount} ${t("chrome.orders")}", color = c.textMuted,
                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 12.sp,
            )
        }
        // Money is the hero — heavy teal, tabular figures (mirrors Swift's .money).
        Text(
            Money.format(d.totalMinor, currency), color = c.accent,
            style = Type.money(18.sp, FontWeight.Black),
        )
        Box(
            Modifier.size(40.dp).clip(RoundedCornerShape(Radii.sm)).background(c.dangerBg)
                .clickable(
                    interactionSource = remember { MutableInteractionSource() }, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress); onDiscard()
                },
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon("trash", tint = c.danger, size = IconSize.md)
        }
    }
}
