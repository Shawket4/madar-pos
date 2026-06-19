// Phase-1 smoke test: proves the rust-core FFI works from Swift.
//
// The generated `sufrix_core.swift` (from ../rust-core/tool/build-bindings.sh)
// must be on this target's source path, and `libsufrix_core` on the link path.
// See ../README.md for the one-liner that compiles + runs this.
import Foundation

let core = try! SufrixCore.fromEnv()   // opens + migrates the local SQLite store
print("✓ core version :", core.version())
print("✓ ffi surface  :", ffiSurfaceVersion())
print("✓ base url     :", core.baseUrl())
print("✓ environment  :", core.environment())
print("✓ greet        :", greet(name: "Teller"))
print("✓ outbox count :", try! core.pendingOutboxCount())   // store reachable over FFI

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

// Session surface — all offline-safe, no server needed. Proves the callback
// interface (TokenStore) and the new auth methods cross the FFI cleanly.
final class MemoryTokenStore: TokenStore {
    var blob: Data?
    func saveBlob(blob: Data) { self.blob = blob }
    func clearBlob() { self.blob = nil }
}
let vault = MemoryTokenStore()
core.setTokenStore(store: vault)
precondition(!core.isAuthenticated(), "fresh core must be signed out")
precondition(core.currentSession() == nil, "no session before login")
print("✓ session      : signed out, token vault installed")

// Offline unlock with no cached bundle must fail cleanly (Swift throw, no crash).
do {
    _ = try core.unlockOffline(name: "Sara", pin: "1234",
                               branchId: "00000000-0000-0000-0000-000000000001")
    preconditionFailure("offline unlock should reject with no cached bundle")
} catch {
    print("✓ offline lock : unlock rejected w/o bundle (\(error))")
}
