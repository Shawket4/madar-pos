// Semantic type scale — a thin naming layer over `Font.ui()/.money()` so screens
// stop sprinkling raw sizes/weights and stay in lockstep with the Compose `Type`
// object. Sizes/weights match the Cairo scale already used across the app
// (headers w700–800, titles w600, body w400–500, money w700 tabular).
import SwiftUI

enum Typo {
    case display // hero numbers / grand totals
    case h1      // screen / hero titles
    case h2      // section / sheet titles
    case h3      // card titles
    case title   // emphasized row titles
    case body    // default body
    case bodySm  // secondary body
    case label   // uppercase section labels
    case labelSm // chips / dense labels
    case money   // amounts (tabular)
    case moneyLg // emphasized totals (tabular)
    case moneyDisplay // grand-total hero amount (tabular)

    // Bolder, more confident scale (the "go bolder" refresh) — hero titles read
    // bigger, card titles step up, and `display` / `moneyDisplay` give grand
    // totals real presence. Kept in lockstep with the Compose `Type` object.
    var font: Font {
        switch self {
        case .display:      return .ui(34, .heavy)
        case .h1:           return .ui(30, .heavy)
        case .h2:           return .ui(22, .bold)
        case .h3:           return .ui(17, .semibold)
        case .title:        return .ui(15, .semibold)
        case .body:         return .ui(14, .medium)
        case .bodySm:       return .ui(13, .regular)
        case .label:        return .ui(12, .semibold)
        case .labelSm:      return .ui(11, .semibold)
        case .money:        return .money(14, .bold)
        case .moneyLg:      return .money(24, .heavy)
        case .moneyDisplay: return .money(34, .heavy)
        }
    }

    /// Tight tracking for the big display/hero styles (matches Compose's negative
    /// letterSpacing). Apply with `.tracking(t.tracking)` on hero Text.
    var tracking: CGFloat {
        switch self {
        case .display, .moneyDisplay: return -0.5
        case .h1: return -0.4
        case .h2: return -0.2
        default:  return 0
        }
    }
}

extension View {
    /// Apply a semantic text style: `Text("Total").typo(.label)`. Hero styles
    /// also get their tight tracking.
    func typo(_ t: Typo) -> some View { font(t.font).tracking(t.tracking) }
}
