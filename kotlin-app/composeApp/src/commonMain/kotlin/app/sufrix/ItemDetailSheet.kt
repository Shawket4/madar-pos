package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.AddonSelection
import app.sufrix.core.ItemAddonView
import app.sufrix.core.MenuItemView
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.StatusChip
import app.sufrix.ui.pressScale
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

private data class Group(
    val id: String,
    val title: String,
    val addons: List<ItemAddonView>,
    val isMulti: Boolean,
    val maxSel: Int?,
    val isRequired: Boolean,
    val minSel: Int,
)

// Item customization — size, addons (per slot + global types), optional fields.
// Prices come pre-resolved from the core (list_item_addons); this only displays
// and sums. Full-screen over the order screen. Mirror of the SwiftUI ItemDetailView.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ItemDetailSheet(model: AppModel, item: MenuItemView, onClose: () -> Unit) {
    val c = sufrixColors()
    val currency = model.session?.currencyCode ?: ""

    var size by remember { mutableStateOf<String?>(null) }
    val single = remember { mutableStateMapOf<String, String>() }     // groupId -> addonId
    var multi by remember { mutableStateOf<Map<String, Map<String, Int>>>(emptyMap()) }
    var optionals by remember { mutableStateOf<Set<String>>(emptySet()) }
    var qty by remember { mutableStateOf(1) }
    var seeded by remember { mutableStateOf(false) }
    // Override: reveal the FULL org addon catalog (every type), not just the
    // item's assigned slots + global types. Mirrors the dashboard's "show all".
    var showAll by remember { mutableStateOf(false) }

    LaunchedEffect(item.id) {
        if (seeded) return@LaunchedEffect
        seeded = true
        val editLine = model.detailEditLine
        if (editLine != null) {
            // Edit mode: reconstruct the selection from the existing line.
            size = editLine.sizeLabel ?: item.sizes.firstOrNull()?.label
            val newMulti = mutableMapOf<String, MutableMap<String, Int>>()
            editLine.addons.forEach { a ->
                val type = model.itemAddons.firstOrNull { it.addonItemId == a.addonItemId }?.addonType ?: return@forEach
                val slot = item.addonSlots.firstOrNull { it.addonType == type }
                if (slot != null) {
                    if ((slot.maxSelections?.toInt() ?: 2) > 1) newMulti.getOrPut(slot.id) { mutableMapOf() }[a.addonItemId] = a.qty.toInt()
                    else single[slot.id] = a.addonItemId
                } else {
                    val gid = "type:$type"
                    if (type != "milk_type") newMulti.getOrPut(gid) { mutableMapOf() }[a.addonItemId] = a.qty.toInt()
                    else single[gid] = a.addonItemId
                }
            }
            multi = newMulti.mapValues { it.value.toMap() }
            optionals = editLine.optionals.map { it.optionalFieldId }.toSet()
            qty = maxOf(1, editLine.qty.toInt())
        } else {
            size = item.sizes.firstOrNull()?.label
            item.defaultMilkAddonId?.let { single["type:milk_type"] = it }
        }
    }

    val milkLabel = t("order.addon_milk_type")
    val coffeeLabel = t("order.addon_coffee_type")
    val extraLabel = t("order.addon_extra")
    fun typeLabel(type: String) = when (type) {
        "milk_type" -> milkLabel; "coffee_type" -> coffeeLabel; "extra" -> extraLabel
        else -> type.split('_').joinToString(" ") { it.replaceFirstChar { ch -> ch.uppercase() } }
    }

    val addonsByType = model.itemAddons.groupBy { it.addonType }
    val slotTypes = item.addonSlots.map { it.addonType }.toSet()
    val baseTypes = listOf("milk_type", "coffee_type", "extra")
    val allowed = item.allowedAddonIds.toSet()
    // Default view shows only the item's allowed addons (the dashboard model);
    // "show all" drops the allowlist filter to reveal the full catalog.
    fun visibleAddons(all: List<ItemAddonView>) =
        if (showAll || allowed.isEmpty()) all else all.filter { it.addonItemId in allowed }
    // "Show all" reveals more: a per-item allowlist hides addons, or a type is off-screen.
    val hasHiddenAddonTypes = allowed.isNotEmpty() || addonsByType.keys.any { it !in slotTypes && it !in baseTypes }
    val groups = buildList {
        item.addonSlots.forEach { s ->
            val addons = visibleAddons(addonsByType[s.addonType] ?: emptyList())
            if (addons.isEmpty()) return@forEach
            val isMulti = (s.maxSelections?.toInt() ?: 2) > 1
            add(Group(s.id, s.label ?: typeLabel(s.addonType), addons, isMulti, s.maxSelections?.toInt(), s.isRequired, s.minSelections.toInt()))
        }
        val extraTypes = if (showAll) baseTypes + addonsByType.keys.filter { it !in baseTypes }.sorted() else baseTypes
        extraTypes.forEach { type ->
            if (type in slotTypes) return@forEach
            val addons = visibleAddons(addonsByType[type] ?: emptyList())
            if (addons.isEmpty()) return@forEach
            add(Group("type:$type", typeLabel(type), addons, type != "milk_type", null, false, 0))
        }
    }

    // ── pricing (display only) ──────────────────────────────────────────────────
    val charged: (String) -> Long = { id -> model.itemAddons.firstOrNull { it.addonItemId == id }?.chargedPriceMinor ?: 0L }
    val unitPrice = size?.let { sz -> item.sizes.firstOrNull { it.label == sz }?.priceMinor } ?: item.basePriceMinor
    val selectedAddons = buildList {
        single.forEach { (_, aid) -> add(AddonSelection(aid, 1)) }
        multi.forEach { (_, m) -> m.forEach { (aid, q) -> add(AddonSelection(aid, q.toLong())) } }
    }
    val addonsTotal = selectedAddons.sumOf { charged(it.addonItemId) * it.qty }
    val optionalsTotal = item.optionalFields.filter { it.id in optionals }.sumOf { it.priceMinor }
    val headerTotal = unitPrice + addonsTotal + optionalsTotal
    val lineTotal = headerTotal * qty
    val firstUnsatisfied = groups.firstOrNull {
        it.isRequired && (if (it.isMulti) (multi[it.id]?.size ?: 0) else (if (single[it.id] != null) 1 else 0)) < maxOf(1, it.minSel)
    }
    val canAdd = firstUnsatisfied == null

    // ── mutations ───────────────────────────────────────────────────────────────
    val toggleSingle: (Group, String) -> Unit = { g, aid ->
        if (single[g.id] == aid) { if (!g.isRequired) single.remove(g.id) } else single[g.id] = aid
    }
    val toggleMulti: (Group, String) -> Unit = { g, aid ->
        val m = (multi[g.id] ?: emptyMap()).toMutableMap()
        if (m.containsKey(aid)) m.remove(aid) else if (g.maxSel == null || m.size < g.maxSel) m[aid] = 1
        multi = multi.toMutableMap().apply { if (m.isEmpty()) remove(g.id) else put(g.id, m) }
    }
    val incMulti: (Group, String) -> Unit = { g, aid ->
        val m = (multi[g.id] ?: emptyMap()).toMutableMap(); m[aid] = (m[aid] ?: 1) + 1
        multi = multi.toMutableMap().apply { put(g.id, m) }
    }
    val decMulti: (Group, String) -> Unit = { g, aid ->
        val m = (multi[g.id] ?: emptyMap()).toMutableMap(); val cur = m[aid] ?: 1
        if (cur <= 1) m.remove(aid) else m[aid] = cur - 1
        multi = multi.toMutableMap().apply { if (m.isEmpty()) remove(g.id) else put(g.id, m) }
    }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        // ── Header ────────────────────────────────────────────────────────────
        Column(Modifier.fillMaxWidth().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Column(Modifier.weight(1f)) {
                    Text(item.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 18.sp)
                    item.description?.takeIf { it.isNotEmpty() }?.let {
                        Text(it, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 12.sp, maxLines = 2)
                    }
                }
                Text(
                    Money.format(headerTotal, currency), color = c.navy, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp,
                    modifier = Modifier.clip(RoundedCornerShape(Radii.sm)).background(c.navyBg).padding(horizontal = 10.dp, vertical = 5.dp),
                )
                Text("✕", color = c.textMuted, fontSize = 16.sp, modifier = Modifier.clickable { onClose() })
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        }

        // ── Content ───────────────────────────────────────────────────────────
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            if (item.sizes.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionTitle(t("order.size"))
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        item.sizes.forEach { s ->
                            SelectChip(s.label, Money.format(s.priceMinor, currency), size == s.label) { size = s.label }
                        }
                    }
                }
            }
            groups.forEach { g ->
                AddonGroupCard(
                    g, currency, charged,
                    selectedSingle = single[g.id],
                    selectedMulti = multi[g.id] ?: emptyMap(),
                    onToggleSingle = { toggleSingle(g, it) },
                    onToggleMulti = { toggleMulti(g, it) },
                    onInc = { incMulti(g, it) },
                    onDec = { decMulti(g, it) },
                )
            }
            if (showAll || hasHiddenAddonTypes) {
                Text(
                    (if (showAll) "▲ " else "＋ ") + t(if (showAll) "order.show_assigned_addons" else "order.show_all_addons"),
                    color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                    modifier = Modifier.fillMaxWidth().clickable { showAll = !showAll }.padding(vertical = Space.sm),
                    textAlign = TextAlign.Center,
                )
            }
            val fields = item.optionalFields.filter { it.isActive }
            if (fields.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionTitle(t("order.optionals"))
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                        fields.forEach { f ->
                            val on = f.id in optionals
                            AddonOptionChip(f.name, f.priceMinor, on, multi = true, currency) {
                                optionals = if (on) optionals - f.id else optionals + f.id
                            }
                        }
                    }
                }
            }
            // Recipe lines for the current size (size-specific + size-agnostic).
            val recipeLines = item.recipes.filter { it.sizeLabel == null || it.sizeLabel == size }
            if (recipeLines.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    SectionTitle(t("order.recipe"))
                    Card {
                        recipeLines.forEachIndexed { i, r ->
                            Row(
                                Modifier.fillMaxWidth().padding(vertical = 9.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(r.ingredientName, color = c.textPrimary, fontFamily = SufrixFont, fontSize = 13.sp)
                                Box(Modifier.weight(1f))
                                Text(
                                    "${fmtQty(r.quantity)} ${r.unit}", color = c.textSecondary,
                                    fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                                )
                            }
                            if (i < recipeLines.size - 1) Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
                        }
                    }
                }
            }
        }

        // ── Footer ──────────────────────────────────────────────────────────────
        Row(
            Modifier.fillMaxWidth().background(c.surface).padding(Space.lg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            MiniStepper(qty, large = true, onDec = { qty = maxOf(1, qty - 1) }, onInc = { qty = minOf(99, qty + 1) })
            val label = if (canAdd) {
                if (model.detailEditKey == null) t("order.add_to_cart") else t("order.update_item")
            } else "${t("order.select_prefix")} ${firstUnsatisfied?.title}"
            Row(
                Modifier.weight(1f).height(50.dp).clip(RoundedCornerShape(Radii.sm))
                    .background(if (canAdd) c.accent else c.accent.copy(alpha = 0.45f))
                    .clickable(enabled = canAdd) {
                        model.addConfigured(item.id, size, selectedAddons, optionals.toList(), qty.toLong(), null)
                    }
                    .padding(horizontal = Space.lg),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(label, color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                Box(Modifier.weight(1f))
                Text(Money.format(lineTotal, currency), color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 14.sp)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AddonGroupCard(
    g: Group,
    currency: String,
    charged: (String) -> Long,
    selectedSingle: String?,
    selectedMulti: Map<String, Int>,
    onToggleSingle: (String) -> Unit,
    onToggleMulti: (String) -> Unit,
    onInc: (String) -> Unit,
    onDec: (String) -> Unit,
) {
    val count = if (g.isMulti) selectedMulti.size else (if (selectedSingle != null) 1 else 0)
    Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
            SectionTitle(g.title)
            if (g.isRequired) StatusChip(t("order.required"), ChipTone.DANGER)
            if (g.isMulti && g.maxSel != null) StatusChip("≤${g.maxSel}", ChipTone.NEUTRAL)
            Box(Modifier.weight(1f))
            if (count > 0) StatusChip("$count", ChipTone.ACCENT)
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            g.addons.forEach { a ->
                val selected = if (g.isMulti) selectedMulti.containsKey(a.addonItemId) else selectedSingle == a.addonItemId
                val price = charged(a.addonItemId)
                if (g.isMulti && selected) {
                    AddonQtyChip(a.name, price, selectedMulti[a.addonItemId] ?: 1, currency, { onDec(a.addonItemId) }, { onInc(a.addonItemId) })
                } else {
                    AddonOptionChip(a.name, price, selected, g.isMulti, currency) {
                        if (g.isMulti) onToggleMulti(a.addonItemId) else onToggleSingle(a.addonItemId)
                    }
                }
            }
        }
    }
}

/** A selectable addon chip (Flutter OptionChip): accent fill when selected. */
@Composable
private fun AddonOptionChip(name: String, price: Long, selected: Boolean, multi: Boolean, currency: String, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier.pressScale(interaction).clip(RoundedCornerShape(Radii.xs))
            .background(if (selected) c.accent else c.surfaceAlt)
            .border(1.dp, if (selected) Color.Transparent else c.border, RoundedCornerShape(Radii.xs))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = 12.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (multi && !selected) {
            Text("＋", color = c.textPrimary.copy(alpha = 0.6f), fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 11.sp)
        }
        Text(name, color = if (selected) c.textOnAccent else c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        if (price > 0) PricePill(price, selected, currency)
    }
}

/** A selected multi-select chip with an inline qty stepper (Flutter QtyChip). */
@Composable
private fun AddonQtyChip(name: String, price: Long, qty: Int, currency: String, onDec: () -> Unit, onInc: () -> Unit) {
    val c = sufrixColors()
    Row(
        Modifier.clip(RoundedCornerShape(Radii.xs)).background(c.accent).padding(horizontal = 4.dp, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        ChipStep("−", onDec)
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(name, color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
            if (price > 0) {
                Text("+${Money.format(price * qty, currency)}", color = c.textOnAccent.copy(alpha = 0.85f), fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 9.sp)
            }
        }
        Text(
            "$qty", color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 11.sp,
            modifier = Modifier.clip(CircleShape).background(c.textOnAccent.copy(alpha = 0.22f)).padding(horizontal = 6.dp, vertical = 2.dp),
        )
        ChipStep("+", onInc)
    }
}

@Composable
private fun ChipStep(glyph: String, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    Box(
        Modifier.size(width = 24.dp, height = 30.dp).clickable {
            haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
        },
        contentAlignment = Alignment.Center,
    ) {
        Text(glyph, color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 13.sp)
    }
}

/** The little "+price" pill inside a chip. */
@Composable
private fun PricePill(price: Long, on: Boolean, currency: String) {
    val c = sufrixColors()
    Text(
        "+${Money.format(price, currency)}", color = if (on) c.textOnAccent else c.accent,
        fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 10.sp,
        modifier = Modifier.clip(CircleShape).background(if (on) c.textOnAccent.copy(alpha = 0.2f) else c.accentBg).padding(horizontal = 6.dp, vertical = 2.dp),
    )
}

/** Recipe quantity: whole numbers without a decimal, else the shortest form. */
private fun fmtQty(q: Double): String =
    if (q == q.toLong().toDouble()) q.toLong().toString() else q.toString()

@Composable
private fun Card(content: @Composable () -> Unit) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(horizontal = Space.md),
    ) { content() }
}

@Composable
private fun SectionTitle(s: String) {
    Text(s.uppercase(), color = sufrixColors().textMuted, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
}

@Composable
private fun SelectChip(label: String, sub: String?, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    Column(
        Modifier.clip(RoundedCornerShape(Radii.sm)).background(if (active) c.accent else c.surface)
            .border(1.dp, if (active) Color.Transparent else c.border, RoundedCornerShape(Radii.sm))
            .clickable { onClick() }.padding(horizontal = Space.lg, vertical = Space.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(label, color = if (active) c.textOnAccent else c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        sub?.let { Text(it, color = if (active) c.textOnAccent.copy(alpha = 0.8f) else c.textSecondary, fontFamily = SufrixFont, fontSize = 11.sp) }
    }
}

@Composable
private fun MiniStepper(value: Int, large: Boolean = false, onDec: () -> Unit, onInc: () -> Unit) {
    val c = sufrixColors()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
        StepBtn("−", onDec)
        Text("$value", color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = if (large) 16.sp else 14.sp, modifier = Modifier.widthIn(min = if (large) 24.dp else 18.dp))
        StepBtn("+", onInc)
    }
}

@Composable
private fun StepBtn(glyph: String, onClick: () -> Unit) {
    val c = sufrixColors()
    Box(
        Modifier.size(30.dp).clip(CircleShape).background(c.surfaceAlt).border(1.dp, c.border, CircleShape).clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Text(glyph, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp)
    }
}
