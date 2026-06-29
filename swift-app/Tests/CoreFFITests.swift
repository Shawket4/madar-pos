// FFI tests for the Rust core consumed from Swift — the client side of the money
// and session engines. Compiled + run by `rust-core/tool/test-swift.sh`, which
// links the generated UniFFI bindings against `libmadar_core` (the same proven
// path as the smoke test) with `-parse-as-library`. Drives the SAME pricing
// engine the property/fuzz tests cover on the Rust side, across the real FFI
// boundary the app uses. Exit code reflects pass/fail.
import Foundation

@main
enum CoreFFITests {
    static func main() {
        var passed = 0
        var failed = 0
        func check(_ name: String, _ cond: Bool) {
            if cond { passed += 1; print("  ✓ \(name)") } else { failed += 1; print("  ✗ \(name)") }
        }
        func line(_ unit: Int64, _ qty: Int64) -> CartLine {
            CartLine(quantity: qty, unitPrice: unit, isBundle: false,
                     addons: [], optionals: [], bundleComponents: [])
        }
        func priced(_ lines: [CartLine], discount: DiscountKind = .none, value: Int64 = 0,
                    tax: Double = 0, tender: Int64? = nil, tip: Int64 = 0) -> PricedBreakdown {
            priceCart(input: PriceCartInput(lines: lines, discountKind: discount, discountValue: value,
                                            taxRate: tax, amountTendered: tender, cashTip: tip))
        }

        print("── Swift core-FFI tests ──")

        // 1. Pricing crosses the FFI and matches the backend money math.
        let b = priced([line(1000, 2)], tax: 0.14, tender: 2500)
        check("pricing subtotal = 2000", b.subtotalMinor == 2000)
        check("pricing tax 14% = 280", b.taxMinor == 280)
        check("pricing total = 2280", b.totalMinor == 2280)
        check("pricing change = 220", b.changeGivenMinor == 220)

        // 2. A >100% percentage discount must NOT drive the total negative (doc 05 F8).
        let d = priced([line(1000, 1)], discount: .percentage, value: 150, tax: 0.1)
        check("discount clamped to subtotal", d.discountMinor <= d.subtotalMinor)
        check("over-discount total >= 0", d.totalMinor >= 0)
        check("over-discount taxable = 0", d.taxableMinor == 0)

        // 3. Change floors at 0 when tender < total.
        check("change floored at 0", priced([line(5000, 1)], tender: 100).changeGivenMinor == 0)

        // 4. Empty cart is a clean zero, not a crash across the FFI.
        check("empty cart total = 0", priced([]).totalMinor == 0)

        // 5. Addons + optionals add into the line total.
        let withExtras = priced([CartLine(quantity: 1, unitPrice: 1000, isBundle: false,
                                          addons: [AddonSel(priceModifier: 250, quantity: 2)],
                                          optionals: [OptionalSel(price: 100)],
                                          bundleComponents: [])])
        check("addons+optionals fold in (1000+500+100)", withExtras.subtotalMinor == 1600)

        // 6. Core + session surface is offline-safe and fails closed.
        let core = try! MadarCore.fromEnv()
        check("core version non-empty", !core.version().isEmpty)
        check("fresh core signed out", !core.isAuthenticated())
        check("no session before login", core.currentSession() == nil)
        do {
            _ = try core.unlockOffline(name: "Sara", pin: "1234",
                                       branchId: "00000000-0000-0000-0000-000000000001")
            check("offline unlock rejects w/o bundle", false)
        } catch {
            check("offline unlock rejects w/o bundle", true)
        }

        print("── \(passed) passed, \(failed) failed ──")
        exit(failed == 0 ? 0 : 1)
    }
}
