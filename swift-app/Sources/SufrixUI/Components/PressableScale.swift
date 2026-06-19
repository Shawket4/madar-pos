// The signature tactile press (Flutter `AnimatedPressScale`): every press
// surface scales down slightly and fires a haptic, instead of a Material ripple.
import SwiftUI

enum Haptics {
    static func selection() {
        #if os(iOS)
        UISelectionFeedbackGenerator().selectionChanged()
        #endif
    }
    static func impact() {
        #if os(iOS)
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        #endif
    }
}

/// Button style: scale on press. Pair with `Haptics.selection()` in the tap
/// action so every press surface buzzes consistently.
struct PressableScaleStyle: ButtonStyle {
    var scale: CGFloat = Motion.pressScale

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? scale : 1)
            .animation(Motion.press, value: configuration.isPressed)
    }
}

extension ButtonStyle where Self == PressableScaleStyle {
    static var pressable: PressableScaleStyle { PressableScaleStyle() }
    static func pressable(scale: CGFloat) -> PressableScaleStyle {
        PressableScaleStyle(scale: scale)
    }
}
