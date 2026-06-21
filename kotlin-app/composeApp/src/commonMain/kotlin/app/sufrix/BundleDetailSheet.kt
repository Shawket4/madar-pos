package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.AddonSelection
import app.sufrix.core.BundleComponentSelection
import app.sufrix.core.BundleComponentView
import app.sufrix.core.BundleView
import app.sufrix.core.MenuItemView
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.pressScale
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t
import coil3.compose.AsyncImage

// Bundle (combo) configuration sheet + the catalog card. A bundle is a fixed
// price covering a set of component items; each component is configured through
// the SAME item-customization sheet (ItemDetailSheet) in "configure mode", which
// returns the selection instead of writing to the cart. "Add to cart" records one
// bundle line via the core (cart_add_bundle), where the component up-charges are
// resolved. Mirror of the SwiftUI BundleDetailView + BundleCard.
@Composable
fun BundleDetailSheet(model: AppModel, bundle: BundleView, onClose: () -> Unit) {
    val c = sufrixColors()
    val currency = model.session?.currencyCode ?: ""
    val components = bundle.components

    // Per-component config, keyed by the component's index (handles a bundle that
    // lists the same item twice).
    val drafts = remember { mutableStateMapOf<Int, BundleComponentDraft>() }
    // Non-null = the per-component customization sheet is open for this index.
    var configuringIndex by remember { mutableStateOf<Int?>(null) }
    var configuringItem by remember { mutableStateOf<MenuItemView?>(null) }

    // A component needs configuring when it has a size choice, addon slots, or
    // active optionals (Flutter's componentNeedsConfiguration).
    fun needsConfig(item: MenuItemView): Boolean =
        item.sizes.size > 1 || item.addonSlots.isNotEmpty() || item.optionalFields.any { it.isActive }
    fun itemFor(comp: BundleComponentView): MenuItemView? = model.menuItems.firstOrNull { it.id == comp.itemId }

    // All configurable components must be configured before adding.
    val canAdd = components.withIndex().all { (idx, comp) ->
        val item = itemFor(comp)
        if (item == null || !needsConfig(item)) true else drafts[idx] != null
    }
    val extrasTotal = drafts.values.sumOf { it.extrasMinor }
    val liveTotal = bundle.priceMinor + extrasTotal

    Box(Modifier.fillMaxSize()) {
        // Tap the dimmed area outside the panel to dismiss.
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onClose() })
        // The sheet panel itself (taps inside are swallowed, not dismissed).
        Column(
            Modifier.fillMaxWidth().fillMaxHeight(0.96f).align(Alignment.BottomCenter)
                .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
        ) {
            // Grab handle — a downward drag past the threshold dismisses the sheet.
            var dragAccum by remember { mutableStateOf(0f) }
            Box(
                Modifier.fillMaxWidth().draggable(
                    orientation = Orientation.Vertical,
                    state = rememberDraggableState { delta -> dragAccum += delta },
                    onDragStopped = { if (dragAccum > 120f) onClose(); dragAccum = 0f },
                ),
                contentAlignment = Alignment.Center,
            ) {
                Box(Modifier.padding(top = 8.dp, bottom = 4.dp).size(width = 36.dp, height = 4.dp)
                    .clip(CircleShape).background(c.borderLight))
            }
            // ── Header (name + description · price badge · close) ─────────────────
            Column(Modifier.fillMaxWidth().background(c.surface)) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(bundle.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 18.sp)
                        bundle.description?.takeIf { it.isNotEmpty() }?.let {
                            Text(it, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
                    Text(
                        Money.format(bundle.priceMinor, currency), color = c.navy, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp,
                        modifier = Modifier.height(32.dp).clip(RoundedCornerShape(Radii.sm)).background(c.navyBg)
                            .padding(horizontal = 10.dp).wrapContentHeight(Alignment.CenterVertically),
                    )
                    Box(
                        Modifier.size(32.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
                            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).clickable { onClose() },
                        contentAlignment = Alignment.Center,
                    ) {
                        Text("✕", color = c.textMuted, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            // ── Content (component checklist) ─────────────────────────────────────
            Column(
                Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                SectionTitle(t("order.bundle_includes"))
                components.forEachIndexed { idx, comp ->
                    val item = itemFor(comp)
                    val configurable = item != null && needsConfig(item)
                    ComponentTile(
                        comp, currency,
                        configurable = configurable,
                        draft = drafts[idx],
                        onClick = {
                            if (item != null && configurable) {
                                // Load the component's addons into the model, then open
                                // the per-component customization sheet.
                                val loaded = model.componentItem(comp.itemId)
                                if (loaded != null) {
                                    configuringIndex = idx
                                    configuringItem = loaded
                                }
                            }
                        },
                    )
                }
            }

            // ── Footer (Add to cart · live total) ─────────────────────────────────
            val label = if (canAdd) t("order.add_to_cart") else t("order.configure")
            Row(
                Modifier.fillMaxWidth().background(c.surface).padding(Space.lg),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(
                    Modifier.weight(1f).height(50.dp).clip(RoundedCornerShape(Radii.sm))
                        .background(if (canAdd) c.accent else c.accent.copy(alpha = 0.45f))
                        .clickable(enabled = canAdd) {
                            val selections = components.mapIndexed { idx, comp ->
                                val d = drafts[idx]
                                val defaultSize = itemFor(comp)?.sizes?.firstOrNull()?.label
                                BundleComponentSelection(
                                    comp.itemId,
                                    d?.sizeLabel ?: defaultSize,
                                    comp.quantity,
                                    d?.addons ?: emptyList<AddonSelection>(),
                                    d?.optionalIds ?: emptyList<String>(),
                                )
                            }
                            model.addBundle(bundle.id, selections)
                        }
                        .padding(horizontal = Space.lg),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(label, color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    Box(Modifier.weight(1f))
                    Text(Money.format(liveTotal, currency), color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 14.sp)
                }
            }
        }

        // Per-component customization, reusing ItemDetailSheet in configure mode.
        val cfgIdx = configuringIndex
        val cfgItem = configuringItem
        if (cfgIdx != null && cfgItem != null) {
            ItemDetailSheet(
                model, cfgItem,
                onClose = { configuringIndex = null; configuringItem = null },
                configureSeed = drafts[cfgIdx],
                onConfigure = { draft ->
                    drafts[cfgIdx] = draft
                    configuringIndex = null
                    configuringItem = null
                },
            )
        }
    }
}

/** A bundle component row — status icon, qty× name + a config summary, the chosen
 *  extras up-charge, and a chevron when configurable. */
@Composable
private fun ComponentTile(
    comp: BundleComponentView,
    currency: String,
    configurable: Boolean,
    draft: BundleComponentDraft?,
    onClick: () -> Unit,
) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val configured = draft != null
    // Status icon: muted ✓ when not configurable, success ✓ once configured, an
    // accent slider glyph while it still needs configuring.
    val (glyph, glyphColor) = when {
        !configurable -> "✓" to c.textMuted
        configured -> "✓" to c.success
        else -> "⋯" to c.accent
    }
    // Subtitle: "Includes" when fixed, the chosen size · +N once configured, else
    // a "Configure" prompt.
    val subtitle = when {
        !configurable -> t("order.bundle_includes")
        !configured -> t("order.configure")
        else -> {
            val parts = mutableListOf<String>()
            draft?.sizeLabel?.let { parts.add(it) }
            val extras = (draft?.addons?.size ?: 0) + (draft?.optionalIds?.size ?: 0)
            if (extras > 0) parts.add("+$extras")
            if (parts.isEmpty()) t("order.configure") else parts.joinToString(" · ")
        }
    }
    Row(
        Modifier.fillMaxWidth().pressScale(interaction, 0.99f).clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, if (configured) c.accent.copy(alpha = 0.4f) else c.border, RoundedCornerShape(Radii.sm))
            .then(
                if (configurable) Modifier.clickable(interactionSource = interaction, indication = null) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
                } else Modifier
            )
            .padding(horizontal = Space.md, vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(Modifier.size(22.dp), contentAlignment = Alignment.Center) {
            Text(glyph, color = glyphColor, fontWeight = FontWeight.Bold, fontSize = 16.sp)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("${comp.quantity}× ${comp.itemName}", color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            Text(subtitle, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 11.sp)
        }
        if (draft != null && draft.extrasMinor > 0) {
            Text("+${Money.format(draft.extrasMinor, currency)}", color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 12.sp)
        }
        if (configurable) {
            Text("›", color = c.textMuted, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
        }
    }
}

@Composable
private fun SectionTitle(s: String) {
    Text(s.uppercase(), color = sufrixColors().textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
}

/** A combo card in the catalog grid — a gradient hero with a Combo chip, the
 *  bundle name, component count, and fixed price. Matches the MenuItemCard style. */
@Composable
fun BundleCard(bundle: BundleView, currency: String, onTap: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Column(
        Modifier.fillMaxWidth().pressScale(interaction, 0.98f)
            .clip(RoundedCornerShape(Radii.md)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.md))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onTap()
            },
    ) {
        // ── Hero (accent gradient + optional photo + Combo chip) ──────────────
        Box(
            Modifier.fillMaxWidth().aspectRatio(1.6f).clipToBounds()
                .background(Brush.linearGradient(listOf(c.accent, c.accent.copy(alpha = 0.7f)))),
        ) {
            val url = bundle.imageUrl
            if (!url.isNullOrBlank()) {
                AsyncImage(
                    model = url,
                    contentDescription = null,
                    modifier = Modifier.matchParentSize().alpha(0.55f),
                    contentScale = ContentScale.Crop,
                )
            }
            Box(Modifier.align(Alignment.TopStart).padding(Space.sm)) {
                StatusChip(t("order.combos"), ChipTone.ACCENT)
            }
        }
        // ── Footer (name · component count · price) ────────────────────────────
        Column(Modifier.fillMaxWidth().background(c.surface).padding(Space.md), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(bundle.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${bundle.components.size} ${t("order.bundle_includes")}", color = c.textSecondary, fontFamily = SufrixFont, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(Money.format(bundle.priceMinor, currency), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 14.sp)
        }
    }
}
