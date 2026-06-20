// Money formatting — minor units → a display string. Kept tiny and identical to
// the Kotlin `Money` so totals read the same on both platforms. (A locale-aware
// formatter can replace this later; the catalog wire is already integer minor.)
import Foundation

enum Money {
    /// "EGP 12.50" — `minor` units rendered with two decimals and the code.
    static func format(_ minor: Int64, _ code: String) -> String {
        let major = Double(minor) / 100
        let amount = String(format: "%.2f", major)
        let c = code.uppercased()
        return c.isEmpty ? amount : "\(c) \(amount)"
    }
}
