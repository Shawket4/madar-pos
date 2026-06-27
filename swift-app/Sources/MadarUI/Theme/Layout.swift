// Responsive layout tokens — one source of truth for breakpoints and content
// max-widths so phone / tablet / desktop behave consistently across every
// screen and match the Compose `Layout` object 1:1. Breakpoints mirror the
// Flutter form-factor rules (sufrix_pos/lib/core/utils/responsive.dart):
// tablet ≥ 600 (shortest side), desktop ≥ 1100 (longest side). The rebuild
// screens read the *container* width (GeometryReader), so these are the
// width thresholds the layouts switch on. Values match what the screens
// already use — naming them prevents drift, not behavior changes.
import SwiftUI

// NOTE: named `Responsive` (not `Layout`) — `Layout` is a SwiftUI protocol
// (custom layouts like our `FlowLayout` conform to it), so an enum named
// `Layout` shadows it and breaks those conformances.
enum Responsive {
    // MARK: Breakpoints (container width)
    static let tablet: CGFloat = 600      // ≥ → tablet spacing / wider forms
    static let wideTable: CGFloat = 680    // ≥ → table layout (history / shifts)
    static let wide: CGFloat = 760         // ≥ → split / side-by-side (login / open-shift / order)
    static let desktop: CGFloat = 1100     // ≥ → desktop: cap + center content

    // MARK: Content max-widths (caps so content centers, never stretches)
    static let formMaxWidth: CGFloat = 520     // phone form column
    static let formMaxWidthWide: CGFloat = 600  // form column on tablet+
    static let listMaxWidth: CGFloat = 560     // single-column lists
    static let contentMaxWidth: CGFloat = 880  // tables / dense screens
    static let sheetMaxWidth: CGFloat = 600    // bottom sheets (Flutter ResponsiveSheet)
    static let sheetCompactMaxWidth: CGFloat = 540 // item / bundle customize sheets

    // MARK: Split ratios (brand panel ↔ form)
    static let brandPanelRatio: CGFloat = 0.55

    /// Form column width for the current container width (scales up off phones).
    static func formWidth(_ width: CGFloat) -> CGFloat {
        width >= tablet ? formMaxWidthWide : formMaxWidth
    }
}
