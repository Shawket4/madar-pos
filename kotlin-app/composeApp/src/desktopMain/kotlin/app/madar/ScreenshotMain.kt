package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.ui.Alignment
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
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import kotlinx.coroutines.asCoroutineDispatcher
import app.madar.core.CartLineView
import app.madar.core.CartTotals
import app.madar.core.MenuItemView
import app.madar.core.PaymentMethodView
import app.madar.core.MadarConfig
import app.madar.core.MadarCore
import app.madar.core.RealtimePlayer
import app.madar.core.SessionSnapshot
import app.madar.core.ShiftView
import app.madar.core.BundleComponentView
import app.madar.core.BundleView
import app.madar.core.OrderSummaryView
import app.madar.core.OutboxItemView
import app.madar.core.DraftView
import app.madar.core.DeliveryOrderView
import app.madar.core.TicketView
import app.madar.core.TicketLineView
import app.madar.core.KdsTicketView
import app.madar.core.KdsLineView
import app.madar.core.KdsStationView
import app.madar.core.TillView
import app.madar.core.ShiftReportView
import app.madar.core.ShiftReportPaymentLine
import app.madar.core.ShiftReportCashLine
import app.madar.core.CashMovementView
import app.madar.core.ShiftSummaryView
import app.madar.core.ItemSizeView
import app.madar.core.AddonSlotView
import app.madar.ui.BtnVariant
import app.madar.ui.ChipTone
import app.madar.ui.MenuItemCard
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
import app.madar.ui.RealtimeAlertData
import app.madar.ui.RealtimeAlertStack
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
    renderOrder(File(outDir, "order-light.png"), MadarLight, family)
    renderOrder(File(outDir, "order-dark.png"), MadarDark, family)
    // Render every REAL screen composable through a real (in-memory core) AppModel,
    // seeded with per-screen stub state, to screen-<name>-{light,dark}.png.
    renderAllRealScreens(outDir, family)
    println("Wrote screenshots to ${outDir.absolutePath}")
}

@OptIn(ExperimentalComposeUiApi::class)
private fun renderScreens(out: File, colors: MadarColors, family: FontFamily) {
    val density = 2f
    val widthDp = 460
    val heightDp = 1480
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

        // ── In-app realtime alert stack (iOS-style deck, companion to the OS notif) ──
        SectionHeader("Live alerts")
        RealtimeAlertStack(
            listOf(
                RealtimeAlertData(3, "New ticket fired · T-12", "Table 4 · 3 items", "ticket.fired:t12"),
                RealtimeAlertData(2, "Order ready · K-88", "Margherita Pizza", "kitchen.ticket_ready:k88"),
                RealtimeAlertData(1, "New delivery order · D-204", "Sara A. · EGP 132.00", "delivery.created:o1"),
            ),
            onDismiss = {},
            onOpen = {},
        )

        // ── Kitchen station picker (commissioning a KDS device) ──────────────
        SectionHeader("Kitchen station picker")
        listOf("Grill" to true, "Fryer" to false, "Cold / Salads" to false).forEach { (name, def) ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.surface)
                    .border(1.dp, c.borderLight, RoundedCornerShape(Radii.md)).padding(Space.lg),
                verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                Box(Modifier.size(40.dp).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg), contentAlignment = androidx.compose.ui.Alignment.Center) {
                    MadarIcon("flame.fill", tint = c.accent, size = IconSize.lg)
                }
                Column(Modifier.weight(1f)) {
                    Text(name, style = Type.h3(), color = c.textPrimary)
                    if (def) Text("Default", style = Type.labelSm(), color = c.textMuted)
                }
                MadarIcon("chevron.forward", tint = c.textMuted, size = IconSize.md)
            }
        }

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

// ── Order screen shell (the refreshed nav-rail layout) ───────────────────────────
// Renders the REAL NavRail (from OrderScreen.kt) + a representative catalog + cart
// with literal data, so the new shell can be eyeballed headlessly with the real
// Cairo face and tokens.
@OptIn(ExperimentalComposeUiApi::class)
private fun renderOrder(out: File, colors: MadarColors, family: FontFamily) {
    val density = 2f
    val widthDp = 1080
    val heightDp = 800
    val scene = ImageComposeScene(
        width = (widthDp * density).toInt(),
        height = (heightDp * density).toInt(),
        density = Density(density),
    ) {
        CompositionLocalProvider(
            LocalMadarColors provides colors,
            LocalMadarFont provides family,
            LocalLocalize provides orderLoc,
        ) {
            MaterialTheme { OrderMock() }
        }
    }
    val img = scene.render()
    val data = img.encodeToData(EncodedImageFormat.PNG) ?: error("PNG encode failed")
    out.writeBytes(data.bytes)
    scene.close()
}

@Composable
private fun OrderMock() {
    val c = madarColors()
    val sections = listOf(
        NavSection("Orders", listOf(
            NavDest("bicycle", "Incoming") {},
            NavDest("tray.full", "Drafts") {},
            NavDest("list.bullet.rectangle", "History") {},
            NavDest("magnifyingglass", "Search") {},
        )),
        NavSection("Money", listOf(
            NavDest("banknote", "Cash") {},
            NavDest("clock.arrow.circlepath", "Shifts") {},
            NavDest("printer", "Report") {},
        )),
    )
    val footer = NavSection("System", listOf(
        NavDest("arrow.triangle.2.circlepath", "Sync") {},
        NavDest("gearshape", "Settings") {},
        NavDest("ellipsis", "More") {},
    ))
    Row(Modifier.fillMaxSize().background(c.bg)) {
        NavRail(sections, footer, Modifier.width(80.dp).fillMaxHeight())
        Box(Modifier.width(1.dp).fillMaxHeight().background(c.border))
        Column(Modifier.weight(1f).fillMaxHeight()) {
            // Top status bar
            Row(
                Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                StatusChip("Omar Khaled", ChipTone.INFO, icon = "person.fill")
                Box(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.surfaceAlt)
                        .border(1.dp, c.borderLight, RoundedCornerShape(Radii.pill)).padding(horizontal = 10.dp, vertical = 5.dp),
                ) {
                    Text("EGP 4,820 · 37 orders", style = Type.labelSm(), color = c.textSecondary)
                }
                Box(Modifier.weight(1f))
                StatusChip("Synced", ChipTone.SUCCESS, icon = "checkmark.circle")
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            // Categories on top
            Row(
                Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Space.lg, vertical = Space.sm),
                horizontalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                SelectableChip("All", true, {})
                SelectableChip("Burgers", false, {})
                SelectableChip("Pizza", false, {})
                SelectableChip("Drinks", false, {})
                SelectableChip("Sides", false, {})
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
            // Menu grid — the REAL MenuItemCard (gradient hero, monogram, in-cart badge).
            Column(
                Modifier.fillMaxWidth().weight(1f).padding(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.md),
            ) {
                listOf(
                    listOf(Triple("Classic beef", 8500L, "Burgers"), Triple("Double smash", 14000L, "Burgers"), Triple("Margherita", 11000L, "Pizza")),
                    listOf(Triple("Iced latte", 6500L, "Coffee"), Triple("Fries", 4000L, "Sides"), Triple("Cheesecake", 7000L, "Bakery")),
                ).forEachIndexed { ri, row ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        row.forEachIndexed { ci, cell ->
                            val (name, price, cat) = cell
                            Box(Modifier.weight(1f)) {
                                MenuItemCard(stubItem("$ri-$ci", name, price), cat, "EGP", if (ri == 0 && ci == 1) 2L else 0L) {}
                            }
                        }
                    }
                }
            }
        }
        Box(Modifier.width(1.dp).fillMaxHeight().background(c.border))
        // Cart — the REAL CartLineRow (qty stepper, tap-to-edit, swipe-to-delete) + CartFooter.
        Column(Modifier.width(340.dp).fillMaxHeight().background(c.surface)) {
            Row(
                Modifier.fillMaxWidth().padding(Space.lg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                Column(Modifier.weight(1f)) {
                    Text("Order #1042", style = Type.h2(), color = c.textPrimary)
                    Text("Table 6 · 2 guests", style = Type.labelSm(), color = c.textMuted)
                }
                StatusChip("Dine-in", ChipTone.ACCENT)
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
            Column(
                Modifier.fillMaxWidth().weight(1f).padding(horizontal = Space.lg, vertical = Space.sm),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                CartLineRow(stubLine("k1", "Double smash", 2L, 14000L), "EGP", {}, {}, onEdit = {}, onSwipeDelete = {})
                CartLineRow(stubLine("k2", "Cheese melt", 1L, 11000L), "EGP", {}, {}, onEdit = {}, onSwipeDelete = {})
                CartLineRow(stubLine("k3", "Cola 330ml", 2L, 2000L), "EGP", {}, {}, onEdit = {}, onSwipeDelete = {})
            }
            CartFooter(CartTotals(5L, 43000L, 0L, 6020L, 49020L), "EGP", onCheckout = {}, onHold = {}, checkoutLabel = "Charge", checkoutIcon = "creditcard")
        }
    }
}

private fun stubItem(id: String, name: String, priceMinor: Long) = MenuItemView(
    id, name, null, "cat", priceMinor, null, true, null,
    emptyList(), emptyList(), emptyList(), emptyList(), emptyList(),
)

private fun stubLine(key: String, name: String, qty: Long, unitMinor: Long) = CartLineView(
    key, "i", name, null, emptyList(), emptyList(), null, unitMinor, qty, unitMinor * qty, null, emptyList(),
)

// English strings for the handful of keys the real cart footer localizes (the
// harness has no core, so LocalLocalize is identity by default).
private val orderLoc: (String) -> String = { k ->
    when (k) {
        "order.subtotal" -> "Subtotal"
        "order.tax" -> "VAT 14%"
        "order.total" -> "Total"
        "order.discount" -> "Discount"
        "order.combos" -> "Combo"
        else -> k
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// REAL screen rendering
//
// Builds a genuine MadarCore against an IN-MEMORY store (dbPath = "") and a real
// AppModel, seeds per-screen UI state directly onto the model, and renders the
// ACTUAL screen composables (LoginScreen, KitchenDisplayScreen, …) headlessly via
// ImageComposeScene — so each refreshed screen can be eyeballed light + dark.
//
// Several AppModel UI-state fields expose `private set` (session, shift, lists, …),
// so the harness seeds them through their generated `<name>$delegate` MutableState
// via reflection (see `seed`). This touches no screen/model source.
//
// Two-pass render: screens reload their lists from the (empty) core in a
// `LaunchedEffect(Unit)` on first composition. We render ONCE to let those effects
// run (and clear our seed to empty), then re-seed and render AGAIN — the Unit-keyed
// effects don't refire on the second frame, so the seeded data survives into the
// saved PNG.
// ─────────────────────────────────────────────────────────────────────────────

/** A no-op realtime player — headless, no beep / notification / haptic. */
private class NoopPlayer : RealtimePlayer {
    override fun playPing() {}
    override fun postNotification(title: String, body: String, tag: String) {}
    override fun haptic() {}
}

/** In-memory host vault (no files) so the harness builds an AppModel cleanly. */
private class MemVault : HostVault {
    private var blob: ByteArray? = null
    override fun saveBlob(blob: ByteArray) { this.blob = blob }
    override fun clearBlob() { blob = null }
    override fun loadBlob(): ByteArray? = blob
    override var branchId: String = ""
    override var branchName: String = ""
    override var orgLogoUrl: String? = null
    override var themeMode: String = ""
    override var locale: String = ""
    override var printerHost: String = ""
    override var printerBrand: String = ""
}

private fun buildCore(): MadarCore = MadarCore(
    MadarConfig(
        baseUrl = "https://api.madar-pos.cloud",
        environment = "dev",
        dbPath = "", // empty ⇒ in-memory store (per MadarConfig docs)
        locale = "en",
    )
)

private fun buildModel(): AppModel = AppModel(buildCore(), MemVault(), NoopPlayer())

/** Write [value] into the AppModel's `<name>` snapshot state, going through the
 *  Kotlin-generated `<name>$delegate` MutableState field. Lets the harness seed
 *  fields whose public setter is `private set` without touching model source. */
@Suppress("UNCHECKED_CAST")
private fun seed(model: AppModel, name: String, value: Any?) {
    val field = AppModel::class.java.getDeclaredField("$name\$delegate")
    field.isAccessible = true
    val state = field.get(model) as androidx.compose.runtime.MutableState<Any?>
    state.value = value
}

// ── stub builders ────────────────────────────────────────────────────────────

private fun stubSession(role: String = "teller") = SessionSnapshot(
    userId = "u1", displayName = "Omar Khaled", role = role, orgId = "org1",
    branchId = "b1", currencyCode = "EGP", taxRate = 0.14, online = true,
    permissionsLoaded = true,
)

private fun stubShift() = ShiftView(
    id = "s1", branchId = "b1", tellerId = "u1", tellerName = "Omar Khaled",
    openingCashMinor = 50000L, openedAt = "2026-06-29T08:00:00Z", status = "open",
    isOpen = true,
)

private fun stubMenuItems(): List<MenuItemView> = listOf(
    Triple("Classic beef burger", 8500L, "Burgers"),
    Triple("Double smash", 14000L, "Burgers"),
    Triple("Margherita pizza", 11000L, "Pizza"),
    Triple("Iced latte", 6500L, "Coffee"),
    Triple("Crispy fries", 4000L, "Sides"),
    Triple("Cheesecake", 7000L, "Bakery"),
).mapIndexed { i, (n, p, _) ->
    MenuItemView(
        "m$i", n, null, "cat", p, null, true, null,
        emptyList(), emptyList(), emptyList(), emptyList(), emptyList(),
    )
}

private fun stubCartLines(): List<CartLineView> = listOf(
    stubLine("k1", "Double smash", 2L, 14000L),
    stubLine("k2", "Crispy fries", 1L, 4000L),
    stubLine("k3", "Iced latte", 1L, 6500L),
)

private fun stubHistory(): List<OrderSummaryView> = listOf(
    OrderSummaryView("o1", 1042, 43000L, 6020L, 49020L, "Cash", "completed", "2026-06-29T11:42:00Z", false, "Omar Khaled", "dine_in", "Table 6", "A-1042"),
    OrderSummaryView("o2", 1041, 21000L, 2940L, 23940L, "Card", "completed", "2026-06-29T11:20:00Z", false, "Omar Khaled", "delivery", "Sara A.", "A-1041"),
    OrderSummaryView("o3", null, 11000L, 1540L, 12540L, "Cash", "queued", "2026-06-29T11:05:00Z", true, null, "dine_in", null, null),
    OrderSummaryView("o4", 1039, 18000L, 2520L, 20520L, "Card", "voided", "2026-06-29T10:40:00Z", false, "Mona R.", "dine_in", "Table 2", "A-1039"),
)

private fun stubOutbox(): List<OutboxItemView> = listOf(
    OutboxItemView("ob1", "create_order", "pending", 0L, null, "2026-06-29T11:05:00Z"),
    OutboxItemView("ob2", "void_order", "inflight", 1L, null, "2026-06-29T10:58:00Z"),
    OutboxItemView("ob3", "close_shift", "dead", 5L, "401 Unauthorized — token expired", "2026-06-29T09:30:00Z"),
)

private fun stubDrafts(): List<DraftView> = listOf(
    DraftView("d1", "11:42", 3L, 32000L, "2026-06-29T11:42:00Z"),
    DraftView("d2", "11:10", 1L, 8500L, "2026-06-29T11:10:00Z"),
    DraftView("d3", "Table 9", 5L, 61000L, "2026-06-29T10:55:00Z"),
)

private fun stubDelivery(): List<DeliveryOrderView> = listOf(
    DeliveryOrderView("dv1", "D-204", "in_mall", "preparing", "Sara A.", "+20 100 555 0192", "Mall of Madar · Gate 3 · Shop 21", "Leave at the door, call on arrival", "Cash on delivery", 12000L, 0L, 1200L, 13200L, 3L, listOf(
        TicketLineView("Beef Burger", 2, "Large", listOf("Extra cheese", "No onion"), 8000L, false),
        TicketLineView("Fries", 1, null, emptyList(), 2000L, false),
        TicketLineView("Cola", 1, null, emptyList(), 2000L, false),
    ), "2026-06-29T11:30:00Z", false),
    DeliveryOrderView("dv2", "D-205", "outside", "received", "Khaled M.", "+20 101 222 8841", "14 Madar Street, Apt 5", null, "Card", 8500L, 1000L, 1500L, 9000L, 2L, listOf(
        TicketLineView("Shawarma Wrap", 2, null, listOf("Garlic sauce"), 7000L, false),
        TicketLineView("Water", 1, null, emptyList(), 1500L, false),
    ), "2026-06-29T11:35:00Z", false),
    DeliveryOrderView("dv3", "D-203", "in_mall", "ready", "Nour H.", "+20 102 999 0011", "Gate 1 · Bench 4", null, null, 22000L, 0L, 0L, 22000L, 4L, listOf(
        TicketLineView("Mixed Grill", 2, null, emptyList(), 16000L, false),
        TicketLineView("Rice", 1, null, emptyList(), 3000L, false),
        TicketLineView("Garden Salad", 1, null, emptyList(), 3000L, false),
    ), "2026-06-29T11:12:00Z", false),
)

private fun stubTickets(): List<TicketView> = listOf(
    TicketView("t1", "T-12", "Table 4", "open", "Walk-in", "Mariam", 3, 28000L, null, "2026-06-29T11:20:00Z", false,
        listOf(
            TicketLineView("Classic beef burger", 2, null, listOf("No onion"), 17000L, false),
            TicketLineView("Iced latte", 1, "Large", emptyList(), 7500L, false),
        )),
    TicketView("t2", "T-14", "Table 7", "ready", "Ahmed", "Omar", 2, 41000L, null, "2026-06-29T11:05:00Z", false,
        listOf(TicketLineView("Margherita pizza", 2, null, emptyList(), 22000L, false))),
    TicketView("t3", null, "Table 2", "queued", null, "Youssef", 4, 15000L, null, "2026-06-29T10:58:00Z", true, emptyList()),
)

private fun stubKds(): List<KdsTicketView> = listOf(
    KdsTicketView("k1", "K-88", "Table 7", 1, "order", "firing", "2026-06-29T11:38:00Z",
        listOf(
            KdsLineView("l1", "Margherita pizza", 2, null, emptyList(), null, "st1", "Grill", false),
            KdsLineView("l2", "Garlic bread", 1, null, emptyList(), "Extra crispy", "st1", "Oven", true),
        )),
    KdsTicketView("k2", "K-89", "Table 4", 2, "open_ticket", "firing", "2026-06-29T11:32:00Z",
        listOf(KdsLineView("l3", "Double smash", 3, null, listOf("No pickles"), null, "st1", "Grill", false))),
)

private fun stubStations(): List<KdsStationView> = listOf(
    KdsStationView("st1", "Grill", true, true, "epson", null, null),
    KdsStationView("st2", "Fryer", false, true, null, null, null),
    KdsStationView("st3", "Cold / Salads", false, true, null, null, null),
)

private fun stubTills(): List<TillView> = listOf(
    TillView("till1", "Drawer 1", true, true),
    TillView("till2", "Drawer 2", false, true),
)

private fun stubShiftReport() = ShiftReportView(
    tellerName = "Omar Khaled", openedAt = "2026-06-29T08:00:00Z", closedAt = null,
    printedAt = "2026-06-29T11:45:00Z", isOpen = true,
    expectedCashMinor = 184000L, openingCashMinor = 50000L,
    openingCashWasEdited = false, openingCashOriginalMinor = null, openingCashEditReason = null,
    closingCashDeclaredMinor = null, totalPaymentsMinor = 482000L, netPaymentsMinor = 470000L,
    voidedAmountMinor = 20520L, cashMovementsNetMinor = 0L, cashInMinor = 0L, cashOutMinor = 0L,
    paymentLines = listOf(
        ShiftReportPaymentLine("Cash", true, 21L, 184000L),
        ShiftReportPaymentLine("Card", false, 16L, 298000L),
    ),
    cashMovements = emptyList(), fromServer = true,
)

private fun stubCashMovements(): List<CashMovementView> = listOf(
    CashMovementView("cm1", 20000L, "Float top-up", "Omar Khaled", "2026-06-29T09:15:00Z"),
    CashMovementView("cm2", -5000L, "Supplier — napkins", "Omar Khaled", "2026-06-29T10:30:00Z"),
)

private fun stubShiftHistory(): List<ShiftSummaryView> = listOf(
    ShiftSummaryView("s1", "Madar Downtown", "Omar Khaled", "2026-06-29T08:00:00Z", null, 50000L, null, null, null, "open", true),
    ShiftSummaryView("s0", "Madar Downtown", "Mona R.", "2026-06-28T08:00:00Z", "2026-06-28T22:00:00Z", 50000L, 178000L, 180000L, -2000L, "closed", false),
)

/** A fully-configurable item for the customization sheet (sizes + an addon slot). */
private fun stubConfigurableItem() = MenuItemView(
    "m0", "Iced latte", "House blend, double shot", "cat", 6500L, null, true, null,
    emptyList(),
    sizes = listOf(
        ItemSizeView("sz1", "Small", 6500L, true),
        ItemSizeView("sz2", "Large", 8500L, true),
    ),
    addonSlots = listOf(
        AddonSlotView("as1", "Milk", "milk", true, 1, 1),
        AddonSlotView("as2", "Extras", "extra", false, 0, 3),
    ),
    optionalFields = emptyList(),
    recipes = emptyList(),
)

private fun stubBundle() = BundleView(
    "bd1", "Burger combo", "Burger + fries + drink", 22000L, null, true,
    null, null, null, null,
    components = listOf(
        BundleComponentView("m0", "Classic beef burger", 1L),
        BundleComponentView("m4", "Crispy fries", 1L),
        BundleComponentView("m3", "Iced latte", 1L),
    ),
)

// ── per-screen seeding + content ──────────────────────────────────────────────

/** One renderable real screen. [seed] populates the model's UI state (mix of public
 *  setters + reflection for `private set` fields). Its closures capture stable stub
 *  instances built once per [realScreens] call, so re-applying the seed each
 *  composition writes reference-equal values — letting Compose's snapshot equality
 *  skip, so the ReseedGate converges instead of recomposing forever. */
private class ScreenSpec(
    val name: String,
    val seed: (AppModel) -> Unit,
    val widthDp: Int = 1080,
    val heightDp: Int = 800,
    val content: @Composable (AppModel) -> Unit,
)

// Each call builds the stub instances ONCE and captures them in the seed closures,
// so reseeding is idempotent (same references). Call once per render pass.
private fun realScreens(): List<ScreenSpec> {
  val session = stubSession(); val kitchenSession = stubSession("kitchen"); val waiterSession = stubSession("waiter")
  val shift = stubShift(); val menu = stubMenuItems(); val cart = stubCartLines()
  val cartTotals = CartTotals(4L, 24500L, 0L, 3430L, 27930L)
  val payMethods = listOf(
    PaymentMethodView("cash", "Cash", true, "cash", "#0D6273"),
    PaymentMethodView("visa", "Visa", false, "credit_card", "#1A5FB4"),
    PaymentMethodView("mc", "Mastercard", false, "credit_card", "#C01C28"),
    PaymentMethodView("wallet", "Wallet", false, "wallet", "#9141AC"),
    PaymentMethodView("instapay", "InstaPay", false, "smartphone", "#2EC27E"),
    PaymentMethodView("gift", "Gift Card", false, "gift_card", "#E5A50A"),
  )
  val history = stubHistory(); val outbox = stubOutbox(); val drafts = stubDrafts()
  val delivery = stubDelivery(); val tickets = stubTickets(); val kds = stubKds()
  val stations = stubStations(); val tills = stubTills(); val report = stubShiftReport()
  val cashMoves = stubCashMovements(); val shiftHist = stubShiftHistory()
  val configItem = stubConfigurableItem(); val bundle = stubBundle()
  return listOf(
    // The hero order screen — tablet (persistent rail) AND phone (rail collapses
    // into the top "options" toggle).
    ScreenSpec("Order", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "menuItems", menu); seed(m, "cartLines", cart); seed(m, "cartTotals", cartTotals)
        seed(m, "shiftSalesMinor", 482000L); seed(m, "shiftOrderCount", 37)
    }) { m -> OrderScreen(m) },

    ScreenSpec("OrderMore", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "menuItems", menu); seed(m, "cartLines", cart); seed(m, "cartTotals", cartTotals)
        seed(m, "shiftSalesMinor", 482000L); seed(m, "shiftOrderCount", 37)
        seed(m, "showMore", true)
    }) { m -> OrderScreen(m) },

    ScreenSpec("OrderPhone", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "menuItems", menu); seed(m, "cartLines", cart); seed(m, "cartTotals", cartTotals)
        seed(m, "shiftSalesMinor", 482000L); seed(m, "shiftOrderCount", 37)
    }, widthDp = 390, heightDp = 840) { m -> OrderScreen(m) },

    ScreenSpec("Login", { m ->
        // Branch configured + signed out ⇒ the teller PIN form.
        seed(m, "branchId", "b1"); seed(m, "branchName", "Madar Downtown")
        seed(m, "session", null)
    }) { m -> LoginScreen(m) },

    ScreenSpec("OpenShift", { m ->
        seed(m, "session", session)
        seed(m, "suggestedOpeningCashMinor", 50000L)
    }) { m -> OpenShiftScreen(m) },

    ScreenSpec("Tender", { m ->
        seed(m, "session", session)
        seed(m, "shift", shift)
        seed(m, "menuItems", menu)
        seed(m, "cartLines", cart)
        seed(m, "cartTotals", cartTotals)
        seed(m, "paymentMethods", payMethods)
    }) { m -> TenderOverlay(m, m.session?.currencyCode ?: "EGP", onClose = {}) },

    ScreenSpec("OrderHistory", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "history", history)
        seed(m, "shiftSalesMinor", 482000L); seed(m, "shiftOrderCount", 37)
    }) { m -> OrderHistoryScreen(m) },

    ScreenSpec("OrderSearch", { m ->
        seed(m, "session", session)
        seed(m, "orderSearchResults", history)
        seed(m, "orderSearchTotal", 128); seed(m, "orderSearchHasMore", true)
    }) { m -> OrderSearchScreen(m) },

    ScreenSpec("CashAndShifts", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "cashMovements", cashMoves)
    }) { m -> CashMovementsScreen(m) },

    ScreenSpec("CloseShift", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "shiftReport", report)
    }) { m -> CloseShiftScreen(m) },

    ScreenSpec("KitchenDisplay", { m ->
        seed(m, "session", kitchenSession)
        seed(m, "kdsStations", stations); seed(m, "kdsTickets", kds)
        seed(m, "realtimeConnected", true)
    }) { m -> KitchenDisplayScreen(m, "st1") },

    ScreenSpec("Settings", { m ->
        seed(m, "session", session)
        seed(m, "branchId", "b1"); seed(m, "branchName", "Madar Downtown")
        seed(m, "tills", tills)
    }) { m -> SettingsScreen(m) },

    ScreenSpec("Sync", { m ->
        seed(m, "session", session)
        seed(m, "outbox", outbox)
        seed(m, "pendingCount", 1); seed(m, "syncFailed", 1); seed(m, "isOnline", false)
    }) { m -> SyncScreen(m) },

    ScreenSpec("Delivery", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "deliveryOrders", delivery)
    }) { m -> DeliveryBody(m) },

    ScreenSpec("Drafts", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "drafts", drafts)
    }) { m -> DraftsScreen(m) },

    ScreenSpec("Incoming", { m ->
        seed(m, "session", session); seed(m, "shift", shift)
        seed(m, "deliveryOrders", delivery); seed(m, "openTickets", tickets)
    }) { m -> IncomingScreen(m) },

    ScreenSpec("Waiter", { m ->
        seed(m, "session", waiterSession); seed(m, "shift", shift)
        seed(m, "openTickets", tickets)
    }) { m -> WaiterTicketsListScreen(m) },

    ScreenSpec("StationPicker", { m ->
        seed(m, "session", kitchenSession)
        seed(m, "branchId", "b1"); seed(m, "branchName", "Madar Downtown")
        seed(m, "kdsStations", stations)
    }) { m -> StationPickerScreen(m) },

    ScreenSpec("Reauth", { m ->
        seed(m, "session", session)
        seed(m, "showReauth", true); seed(m, "syncAuthPaused", true)
    }) { m -> ReauthScreen(m) },

    ScreenSpec("ItemDetail", { m ->
        seed(m, "session", session); seed(m, "menuItems", menu)
    }) { m -> ItemDetailSheet(m, configItem, onClose = {}) },

    ScreenSpec("BundleDetail", { m ->
        seed(m, "session", session); seed(m, "menuItems", menu)
    }) { m -> BundleDetailSheet(m, bundle, onClose = {}) },
  )
}

@OptIn(ExperimentalComposeUiApi::class)
private fun renderAllRealScreens(outDir: File, family: FontFamily) {
    val ok = StringBuilder()
    val fail = StringBuilder()
    for (spec in realScreens()) {
        for ((suffix, colors) in listOf("light" to MadarLight, "dark" to MadarDark)) {
            val out = File(outDir, "screen-${spec.name}-$suffix.png")
            runCatching { renderRealScreen(out, colors, family, spec) }
                .onSuccess { ok.append("  ${spec.name} ($suffix) -> ${out.name} [${out.length()}B]\n") }
                .onFailure { fail.append("  ${spec.name} ($suffix): ${it::class.simpleName}: ${it.message}\n") }
        }
    }
    println("Real screens rendered:\n$ok")
    if (fail.isNotEmpty()) println("Real screens FAILED:\n$fail")
}

@OptIn(ExperimentalComposeUiApi::class)
private fun renderRealScreen(out: File, colors: MadarColors, family: FontFamily, spec: ScreenSpec) {
    val density = 2f
    val widthDp = spec.widthDp
    val heightDp = spec.heightDp
    val model = buildModel()
    // Pre-seed BEFORE the scene exists, so the very first composition already sees
    // the stub state. A `ReseedGate` (below) then re-applies the seed once per
    // composition AFTER each screen's own `LaunchedEffect(Unit)` reload, so the
    // reload's empty result is overwritten and the seeded data wins on the next
    // frame. Single `render()` (with `sendApplyNotifications` + a clock tick between
    // composition passes) — never a reentrant render, which deadlocks Compose's
    // FlushCoroutineDispatcher.
    spec.seed(model)
    // Give EACH scene its own single-thread dispatcher (closed below). The default
    // ImageComposeScene shares process-global recomposition machinery, so a screen's
    // still-running `LaunchedEffect` / `rememberCoroutineScope` coroutines from a
    // previous scene held a lock the next scene's render() blocked on — a cross-scene
    // deadlock in FlushCoroutineDispatcher. An isolated context per scene removes that
    // contention; we shut it down after rendering so nothing leaks into the next.
    val executor = java.util.concurrent.Executors.newSingleThreadExecutor()
    val dispatcher = executor.asCoroutineDispatcher()
    val scene = ImageComposeScene(
        width = (widthDp * density).toInt(),
        height = (heightDp * density).toInt(),
        density = Density(density),
        coroutineContext = dispatcher,
    ) {
        CompositionLocalProvider(
            LocalMadarColors provides colors,
            LocalMadarFont provides family,
            LocalLocalize provides { model.t(it) },
            LocalLayoutDirection provides if (model.isRTL) LayoutDirection.Rtl else LayoutDirection.Ltr,
        ) {
            MaterialTheme {
                Box(Modifier.fillMaxSize().background(colors.bg)) { spec.content(model) }
            }
        }
    }
    try {
        // Pass 1 lets each screen's entry `LaunchedEffect(Unit) { model.loadX() }`
        // reload its list from the (empty) in-memory core (clobbering the seed to
        // empty). Re-seed, then render the SAVED frame — the Unit-keyed effects don't
        // refire, so the stub data survives. Sequential renders on this scene's OWN
        // isolated dispatcher don't deadlock (the hang was cross-scene, now removed).
        scene.render(0L)
        spec.seed(model)
        // The pass-1 reload of online-only screens (Delivery, OrderSearch, …) fails
        // against the empty offline core and parks a "not signed in" message in the
        // error slot; clear it so the seeded screen renders clean.
        model.error = null
        val img = scene.render(16_000_000L)
        val data = img.encodeToData(EncodedImageFormat.PNG) ?: error("PNG encode failed")
        out.writeBytes(data.bytes)
    } finally {
        scene.close()
        dispatcher.close()
        executor.shutdownNow()
    }
}
