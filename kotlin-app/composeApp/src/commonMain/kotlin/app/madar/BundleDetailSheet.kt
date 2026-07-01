package app.madar

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.AddonSelection
import app.madar.core.BundleComponentSelection
import app.madar.core.BundleComponentView
import app.madar.core.BundleView
import app.madar.core.MenuItemView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.MadarButton
import app.madar.ui.Money
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.StatusChip
import app.madar.ui.LocalMadarFont
import app.madar.ui.pressScale
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.Elevation
import app.madar.ui.elevation
import app.madar.ui.MotionSpec
import coil3.compose.AsyncImage
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

// Bundle (combo) configuration sheet + the catalog card. A bundle is a fixed
// price covering a set of component items; each component is configured through
// the SAME item-customization sheet (ItemDetailSheet) in "configure mode", which
// returns the selection instead of writing to the cart. "Add to cart" records one
// bundle line via the core (cart_add_bundle), where the component up-charges are
// resolved. Mirror of the SwiftUI BundleDetailView + BundleCard.
@Composable
fun BundleDetailSheet(model: AppModel, bundle: BundleView, onClose: () -> Unit) {
    val c = madarColors()
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

    // Animated present/dismiss (slide-up + scrim fade) — mirrors ItemDetailSheet
    // (was an instant pop). requestClose animates OUT before the parent unmounts.
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val sheetScope = rememberCoroutineScope()
    val scrimAlpha by animateFloatAsState(if (shown) 0.45f else 0f, MotionSpec.standard(), label = "bundleScrim")
    fun requestClose() { shown = false; sheetScope.launch { delay(240); onClose() } }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val distPx = with(LocalDensity.current) { maxHeight.toPx() }
        val slideOff by animateFloatAsState(if (shown) 0f else distPx, MotionSpec.sheet(), label = "bundleSlide")
        // Tap the dimmed area outside the panel to dismiss.
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = scrimAlpha))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { requestClose() })
        // The sheet panel itself (taps inside are swallowed, not dismissed). Capped
        // to a focused width (~520–540) and centered on wide windows. Mirrors
        // ItemDetailSheet; capped at 92% (was a force-filled 96%).
        Column(
            Modifier.widthIn(max = 540.dp).fillMaxWidth().fillMaxHeight(0.92f).align(Alignment.BottomCenter)
                .offset { IntOffset(0, slideOff.roundToInt()) }
                .clip(RoundedCornerShape(topStart = Radii.xl, topEnd = Radii.xl)).background(c.surfaceAlt)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
        ) {
            // Grab handle — white (surface) so it reads continuous with the header.
            var dragAccum by remember { mutableStateOf(0f) }
            Box(
                Modifier.fillMaxWidth().background(c.surface).draggable(
                    orientation = Orientation.Vertical,
                    state = rememberDraggableState { delta -> dragAccum += delta },
                    onDragStopped = { if (dragAccum > 120f) requestClose(); dragAccum = 0f },
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
                        Text(bundle.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 18.sp)
                        bundle.description?.takeIf { it.isNotEmpty() }?.let {
                            Text(it, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
                    Text(
                        Money.format(bundle.priceMinor, currency), color = c.navy, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp,
                        modifier = Modifier.height(32.dp).clip(RoundedCornerShape(Radii.sm)).background(c.navyBg)
                            .padding(horizontal = 10.dp).wrapContentHeight(Alignment.CenterVertically),
                    )
                    Box(
                        Modifier.size(32.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
                            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).clickable { requestClose() },
                        contentAlignment = Alignment.Center,
                    ) {
                        MadarIcon("xmark", tint = c.textMuted, size = IconSize.sm)
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            }

            // ── Content (component checklist) ─────────────────────────────────────
            Column(
                Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.md),
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

            // ── Footer (base + extras · tinted teal total · Add to cart) ──────────
            BundleFooter(
                bundlePriceMinor = bundle.priceMinor,
                extrasMinor = extrasTotal,
                liveTotalMinor = liveTotal,
                currency = currency,
                canAdd = canAdd,
                onAdd = {
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
                },
                modifier = Modifier.fillMaxWidth(),
            )
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

/** The sheet footer — a base + extras breakdown above a tinted-teal grand-total
 *  block (the live combo price is the hero figure), then the Add-to-cart CTA. The
 *  caller owns the footer's width via [modifier]; the footer paints its own
 *  surface, hairline, and padding. Mirrors the Order screen's CartFooter. */
@Composable
private fun BundleFooter(
    bundlePriceMinor: Long,
    extrasMinor: Long,
    liveTotalMinor: Long,
    currency: String,
    canAdd: Boolean,
    onAdd: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    // surface-painted footer with a top hairline; the inner column owns padding.
    Column(modifier.background(c.surface)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        Column(
            Modifier.fillMaxWidth().padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            // Base price + (optional) extras — light sub-rows so the total carries weight.
            TotalRow(t("order.subtotal"), Money.format(bundlePriceMinor, currency))
            if (extrasMinor > 0) {
                TotalRow(t("order.addon_extra"), "+${Money.format(extrasMinor, currency)}")
            }
            // Grand total — tinted teal block, the figure the cashier reads.
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
                    .padding(horizontal = Space.md, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(t("order.total"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                Box(Modifier.weight(1f))
                Text(Money.format(liveTotalMinor, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
            }
            MadarButton(
                label = if (canAdd) t("order.add_to_cart") else t("order.configure"),
                onClick = onAdd,
                enabled = canAdd,
                modifier = Modifier.padding(top = Space.xs),
            )
        }
    }
}

/** A light subtotal/extras row above the tinted total block. */
@Composable
private fun TotalRow(label: String, value: String) {
    val c = madarColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Text(value, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
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
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val configured = draft != null
    // Leading status tile — a tone tile behind the glyph (the design-language
    // tone-tile pattern). A navy "included" ✓ when fixed, success ✓ once
    // configured, an accent slider glyph (on accentBg) while it still needs configuring.
    val (glyph, glyphColor, tileBg) = when {
        !configurable -> Triple("checkmark.circle.fill", c.navy, c.navyBg)
        configured -> Triple("checkmark.circle.fill", c.success, c.successBg)
        else -> Triple("slider.horizontal.3", c.accent, c.accentBg)
    }
    // Subtitle for configurable rows only: a "Configure" prompt, or the chosen
    // size · +N once configured. Fixed components carry no subtitle — the section
    // header already reads "Includes", so a per-row repeat is dead weight.
    val subtitle = when {
        !configurable -> null
        !configured -> t("order.configure")
        else -> {
            val parts = mutableListOf<String>()
            draft?.sizeLabel?.let { parts.add(it) }
            val extras = (draft?.addons?.size ?: 0) + (draft?.optionalIds?.size ?: 0)
            if (extras > 0) parts.add("+$extras")
            if (parts.isEmpty()) t("order.configure") else parts.joinToString(" · ")
        }
    }
    val shape = RoundedCornerShape(Radii.md)
    Row(
        Modifier.fillMaxWidth().pressScale(interaction, 0.99f).elevation(Elevation.CARD, shape).clip(shape).background(c.surface)
            .border(1.dp, if (configured) c.accent.copy(alpha = 0.4f) else c.borderLight, shape)
            .then(
                if (configurable) Modifier.clickable(interactionSource = interaction, indication = null) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
                } else Modifier
            )
            .padding(horizontal = Space.md, vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(
            Modifier.size(40.dp).clip(RoundedCornerShape(Radii.sm)).background(tileBg),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon(glyph, tint = glyphColor, size = IconSize.md)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("${comp.quantity}× ${comp.itemName}", color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
            subtitle?.let {
                Text(it, color = if (configured) c.accent else c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = if (configured) FontWeight.SemiBold else FontWeight.Medium, fontSize = 12.sp)
            }
        }
        if (draft != null && draft.extrasMinor > 0) {
            Text("+${Money.format(draft.extrasMinor, currency)}", color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
        }
        if (configurable) {
            MadarIcon("chevron.right", tint = c.textMuted, size = IconSize.sm)
        }
    }
}

@Composable
private fun SectionTitle(s: String) {
    Text(s.uppercase(), color = madarColors().textMuted, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
}

/** A combo card in the catalog grid — a gradient hero with a Combo chip, the
 *  bundle name, component count, and fixed price. Matches the MenuItemCard style. */
@Composable
fun BundleCard(bundle: BundleView, currency: String, onTap: () -> Unit) {
    val c = madarColors()
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
                StatusChip(t("order.combos"), ChipTone.ACCENT, icon = "bag.fill")
            }
        }
        // ── Footer (name · component count · price) ────────────────────────────
        Column(Modifier.fillMaxWidth().background(c.surface).padding(Space.md), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(bundle.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${bundle.components.size} ${t("order.bundle_includes")}", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(Money.format(bundle.priceMinor, currency), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 14.sp)
        }
    }
}
