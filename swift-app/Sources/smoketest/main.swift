// Phase-1 smoke test: proves the rust-core FFI works from Swift.
//
// The generated `sufrix_core.swift` (from ../rust-core/tool/build-bindings.sh)
// must be on this target's source path, and `libsufrix_core` on the link path.
// See ../README.md for the one-liner that compiles + runs this.
import Foundation

let core = SufrixCore.fromEnv()
print("✓ core version :", core.version())
print("✓ ffi surface  :", ffiSurfaceVersion())
print("✓ base url     :", core.baseUrl())
print("✓ environment  :", core.environment())
print("✓ greet        :", greet(name: "Teller"))

// Exercise the pricing engine across the FFI: 2 × 1000 piastres @ 14% tax,
// tender 2500. Expect subtotal 2000, tax 280, total 2280, change 220.
let breakdown = priceCart(input: PriceCartInput(
    lines: [CartLine(quantity: 2, unitPrice: 1000, isBundle: false,
                     addons: [], optionals: [], bundleComponents: [])],
    discountKind: DiscountKind.none,
    discountValue: 0,
    taxRate: 0.14,
    amountTendered: 2500,
    cashTip: 0))
print("✓ pricing      : subtotal=\(breakdown.subtotalMinor) tax=\(breakdown.taxMinor) total=\(breakdown.totalMinor) change=\(breakdown.changeGivenMinor)")
precondition(breakdown.totalMinor == 2280 && breakdown.changeGivenMinor == 220, "pricing FFI mismatch")
