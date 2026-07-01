// Swift visual-verification harness — the mirror of the Kotlin ScreenshotMain.kt
// (composeApp :screenshots task). Renders the REAL refreshed SwiftUI screens to
// PNG headlessly, with no Xcode, no simulator, no on-screen window.
//
// How it works:
//   • It compiles AGAINST the MadarUI sources + the generated UniFFI binding and
//     links libmadar_core (see tool/screenshot-swift.sh for the exact flags).
//   • AppModel() is constructible standalone offline — it opens a local SQLite
//     core (same path the smoketest exercises). We build one, seed its cart via
//     the real public mutators (which write through the core), wrap a screen in
//     ThemedRoot + the localize environment, and rasterize it with SwiftUI's
//     ImageRenderer.
//   • The Cairo faces are registered here (the app's own registerFonts() reads
//     Bundle.main, which a bare swiftc executable has no resources in) so
//     Font.custom("Cairo-…") resolves and money/headers render in-brand.
//
// It NEVER imports a private symbol and only touches AppModel's public surface,
// so it can't drift the MadarUI type-check. Output: PNGs under
// rust-core/target/swift-screenshots/.
//
//   ./tool/screenshot-swift.sh
import AppKit
import CoreText
import SwiftUI

// MARK: - Environment (paths handed in by the shell script)

private let fontDir = ProcessInfo.processInfo.environment["MADAR_FONT_DIR"]
    ?? "../swift-app/Resources/Fonts"
private let outDir = ProcessInfo.processInfo.environment["MADAR_OUT_DIR"]
    ?? "target/swift-screenshots"

// MARK: - Font registration (Cairo — money is the hero, so this matters)

@MainActor private func registerCairo() {
    let faces = ["Cairo-Regular", "Cairo-Medium", "Cairo-SemiBold", "Cairo-Bold", "Cairo-ExtraBold"]
    for face in faces {
        let path = "\(fontDir)/\(face).ttf"
        guard FileManager.default.fileExists(atPath: path) else {
            FputsErr("  ! missing font \(path)")
            continue
        }
        var err: Unmanaged<CFError>?
        let url = URL(fileURLWithPath: path) as CFURL
        if !CTFontManagerRegisterFontsForURL(url, .process, &err) {
            // Already-registered is benign; anything else we surface but continue.
            FputsErr("  ~ font \(face): \(err.map { String(describing: $0.takeRetainedValue()) } ?? "already registered")")
        }
    }
}

private func FputsErr(_ s: String) { FileHandle.standardError.write((s + "\n").data(using: .utf8)!) }

/// Print + flush immediately — stdout is block-buffered when piped (not a TTY),
/// which otherwise hides progress until the (possibly hanging) process exits.
private func log(_ s: String) { print(s); fflush(stdout) }

// MARK: - Rendering

/// Rasterize a SwiftUI view to a PNG at `path`, at the given logical size and
/// 2× scale. Runs on the main actor (ImageRenderer + AppKit require it).
@MainActor private func render<V: View>(_ view: V, size: CGSize, to path: String) {
    let name = URL(fileURLWithPath: path).lastPathComponent
    log("  · rendering \(name)…")
    let renderer = ImageRenderer(content:
        view
            .frame(width: size.width, height: size.height)
            .environment(\.colorScheme, .light) // overridden by ThemedRoot per-mode
    )
    renderer.scale = 2.0
    renderer.proposedSize = ProposedViewSize(size)
    guard let cg = renderer.cgImage else {
        FputsErr("  ✗ render produced no image: \(name)")
        return
    }
    let rep = NSBitmapImageRep(cgImage: cg)
    guard let png = rep.representation(using: .png, properties: [:]) else {
        FputsErr("  ✗ PNG encode failed: \(name)")
        return
    }
    do {
        try png.write(to: URL(fileURLWithPath: path))
        let bytes = ((try? FileManager.default.attributesOfItem(atPath: path)[.size]) as? Int) ?? 0
        log("  ✓ \(name) (\(bytes) bytes)")
    } catch {
        FputsErr("  ✗ write failed \(name): \(error)")
    }
}

/// One screen, wrapped exactly the way the real app wraps it (ThemedRoot +
/// localize + RTL), for a given theme mode. Takes a pre-built screen view so the
/// builder closure never has to escape.
@MainActor private func themed<V: View>(_ app: AppModel, mode: ThemeMode, _ screen: V) -> some View {
    let t = app.t
    let rtl = app.isRTL
    return ThemedRoot(mode: mode) {
        screen
            .environment(\.localize, { t($0) })
            .environment(\.layoutDirection, rtl ? .rightToLeft : .leftToRight)
    }
}

// MARK: - Stub data

/// A literal menu item (no catalog needed) used to seed the cart through the
/// real core mutator so the cart panel / totals block render with content.
private func stubItem(_ id: String, _ name: String, _ priceMinor: Int64) -> MenuItemView {
    MenuItemView(
        id: id, name: name, description: nil, categoryId: "cat", basePriceMinor: priceMinor,
        imageUrl: nil, isActive: true, defaultMilkAddonId: nil, allowedAddonIds: [],
        sizes: [], addonSlots: [], optionalFields: [], recipes: []
    )
}

@MainActor private func seedOrderCart(_ app: AppModel) {
    // Real mutators → the lines + totals land in the core's SQLite and the
    // @Published mirrors update. No catalog rows required (cartAdd takes the
    // name + price directly), so this works on a cold offline store.
    app.clearCart()
    app.addToCart(stubItem("k1", "Double smash", 14000))
    app.addToCart(stubItem("k2", "Cheese melt", 11000))
    app.setCartQty("k1", 2)
    app.addToCart(stubItem("k3", "Cola 330ml", 2000))
    app.setCartQty("k3", 2)
}

/// A bare BundleView (no catalog needed). Components empty — the configure rows
/// resolve menu items from the catalog, which a cold offline store doesn't have,
/// so the sheet renders its header / price / empty component list.
private func stubBundle() -> BundleView {
    BundleView(
        id: "b1", name: "Combo meal", description: "Burger + fries + drink",
        priceMinor: 18000, imageUrl: nil, isAvailable: true,
        availableFromDate: nil, availableUntilDate: nil,
        availableFromTime: nil, availableUntilTime: nil, components: []
    )
}

/// Park the current cart into a held draft so DraftsView has a row, then re-seed
/// the live cart. Hold no-ops on an empty cart, so seed first.
@MainActor private func seedDraft(_ app: AppModel) {
    seedOrderCart(app)
    app.holdCart()       // parks the cart → one draft row; writes through the core
    seedOrderCart(app)   // restore a live cart for the screens that read it
}

// MARK: - Entry

@main
struct ScreenshotHarness {
    static func main() {
        MainActor.assumeIsolated { run() }
    }
}

@MainActor private func run() {
    // A minimal app context so SwiftUI's renderer + AppKit image stack are alive.
    let appKit = NSApplication.shared
    appKit.setActivationPolicy(.prohibited)

    registerCairo()
    try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

    log("── Swift screenshot harness")
    log("   fonts : \(fontDir)")
    log("   out   : \(outDir)")

    // One real, offline AppModel (opens the local SQLite core).
    log("   building AppModel…")
    let app = AppModel()
    log("   AppModel ready")

    let size = CGSize(width: 1080, height: 800)

    /// Render one screen in both themes to `<name>-light.png` / `<name>-dark.png`.
    /// Takes a builder so each screen is freshly constructed per mode (some hold
    /// @State seeded from the model at init).
    @MainActor func shot<V: View>(_ name: String, _ build: () -> V) {
        render(themed(app, mode: .light, build()), size: size, to: "\(outDir)/\(name)-light.png")
        render(themed(app, mode: .dark,  build()), size: size, to: "\(outDir)/\(name)-dark.png")
    }

    let noop: () -> Void = {}

    // ── Login (device-setup / teller) — simplest, rendered first.
    shot("login") { LoginView(app: app) }

    // ── Open-shift (the drawer-count gate after sign-in).
    shot("openshift") { OpenShiftView(app: app) }

    // ── Station picker (KDS commissioning brand-panel split).
    shot("stationpicker") { StationPickerView(app: app) }

    // ── Re-auth prompt (token expired mid-shift).
    shot("reauth") { ReauthView(app: app, onClose: noop) }

    // ── Order screen (wide desktop layout) — seed the cart so totals render.
    seedOrderCart(app)
    log("   cart seeded (\(app.cartLines.count) lines, total \(app.cartTotals.totalMinor))")
    shot("order") { OrderView(app: app) }

    // ── Tender (checkout sheet) — reads the seeded cart total.
    shot("tender") { TenderView(app: app, onClose: noop) }

    // ── Item-detail customization sheet (stub item; addon slots empty offline).
    shot("itemdetail") { ItemDetailView(app: app, item: stubItem("k1", "Double smash", 14000), onClose: noop) }

    // ── Bundle (combo) configuration sheet.
    shot("bundledetail") { BundleDetailView(app: app, bundle: stubBundle(), onClose: noop) }

    // ── Order history (current shift) — empty/queued list + chrome.
    shot("orderhistory") { OrderHistoryView(app: app, onClose: noop) }

    // ── All-orders search (date range + status filters).
    shot("ordersearch") { OrderSearchView(app: app, onClose: noop) }

    // ── Cash In/Out (the CashAndShifts movements screen).
    shot("cashandshifts") { CashMovementsView(app: app, onClose: noop) }

    // ── Close-shift (count the drawer, end the shift).
    shot("closeshift") { CloseShiftView(app: app) }

    // ── Kitchen display board (stub station id; tickets empty offline).
    shot("kitchendisplay") { KitchenDisplayView(app: app, stationId: "station-1") }

    // ── Settings (appearance / language / device / diagnostics).
    shot("settings") { SettingsView(app: app, onClose: noop) }

    // ── Sync center (durable outbox) — the seeded cart queued commands.
    app.loadOutbox()
    log("   outbox loaded (\(app.outbox.count) entries)")
    shot("sync") { SyncView(app: app, onClose: noop) }

    // ── Delivery queue body (the Delivery tab of the unified Orders surface).
    shot("delivery") { DeliveryBody(app: app) }

    // ── Incoming (unified Orders: delivery + waiter tabs).
    shot("incoming") { IncomingView(app: app, onClose: noop) }

    // ── Waiter open-tickets list.
    shot("waiter") { WaiterTicketsListView(app: app, onClose: noop) }

    // ── Drafts (held orders) — seed one parked draft so a row renders.
    seedDraft(app)
    log("   drafts seeded (\(app.drafts.count))")
    shot("drafts") { DraftsView(app: app, onClose: noop) }

    log("── done")
}

