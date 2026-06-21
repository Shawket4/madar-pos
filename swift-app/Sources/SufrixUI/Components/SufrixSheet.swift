// Custom modal bottom-sheet presentation.
//
// SwiftUI's `.sheet` on macOS is a rigid modal panel: no drag-to-dismiss, no
// tap-outside-to-dismiss, and stacking many on one node leaves dismiss artifacts
// (the "weird artifacting after the orders popup"). This presents a scrim + a
// bottom-anchored card entirely within the view tree, so it:
//   • inherits the environment (theme/localize/RTL/toast) — no modalChrome needed,
//   • supports tap-scrim + drag-down dismissal like Flutter's showModalBottomSheet,
//   • renders identically on macOS and iOS.
//
// Use `.sufrixSheet(isPresented:)` / `.sufrixSheet(item:)` exactly like `.sheet`.
import SwiftUI

/// How tall the card may grow. `.auto` fits its content up to a cap; `.large`
/// reaches ~94% of the container (for big sheets like item-detail / checkout).
enum SheetSize { case auto, large }

extension View {
    /// Present a custom bottom sheet driven by a boolean. The content builder gets
    /// an animated `dismiss` closure — wire it to the content's close button so the
    /// card slides out (not a hard cut).
    func sufrixSheet<SheetBody: View>(
        isPresented: Binding<Bool>,
        size: SheetSize = .auto,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder content: @escaping (_ dismiss: @escaping () -> Void) -> SheetBody
    ) -> some View {
        overlay {
            if isPresented.wrappedValue {
                SufrixSheetContainer(size: size, requestClose: {
                    isPresented.wrappedValue = false
                    onDismiss?()
                }) { dismiss in content(dismiss) }
            }
        }
    }

    /// Present a custom bottom sheet driven by an optional identifiable item.
    func sufrixSheet<Item: Identifiable, SheetBody: View>(
        item: Binding<Item?>,
        size: SheetSize = .auto,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder content: @escaping (_ item: Item, _ dismiss: @escaping () -> Void) -> SheetBody
    ) -> some View {
        overlay {
            if let value = item.wrappedValue {
                SufrixSheetContainer(size: size, requestClose: {
                    item.wrappedValue = nil
                    onDismiss?()
                }) { dismiss in content(value, dismiss) }
            }
        }
    }
}

extension View {
    /// Present a full-screen ROUTED screen (not a sheet) that slides in from the
    /// trailing edge over the order hub — the macOS-safe equivalent of a navigation
    /// push (macOS has no `fullScreenCover`). The content fills the window and
    /// supplies its own back-chevron header wired to `dismiss`. Keeps the design
    /// rule that the order action bar is the only nav hub: the bar pushes screens.
    func appScreen<ScreenBody: View>(
        isPresented: Binding<Bool>,
        @ViewBuilder content: @escaping (_ dismiss: @escaping () -> Void) -> ScreenBody
    ) -> some View {
        modifier(AppScreenModifier(isPresented: isPresented, screen: content))
    }
}

private struct AppScreenModifier<ScreenBody: View>: ViewModifier {
    @Environment(\.theme) private var theme
    @Binding var isPresented: Bool
    @ViewBuilder var screen: (_ dismiss: @escaping () -> Void) -> ScreenBody

    func body(content: Content) -> some View {
        content.overlay {
            ZStack {
                if isPresented {
                    screen { withAnimation(Motion.standard) { isPresented = false } }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(theme.colors.bg.ignoresSafeArea())
                        .transition(.move(edge: .trailing))
                        .zIndex(20)
                }
            }
            .animation(Motion.standard, value: isPresented)
        }
    }
}

/// The scrim + sliding card. Owns its enter/exit animation so user dismissals
/// (tap-scrim, drag-down, close button) animate out before the binding clears.
private struct SufrixSheetContainer<SheetBody: View>: View {
    @Environment(\.theme) private var theme
    let size: SheetSize
    let requestClose: () -> Void
    @ViewBuilder var sheetBody: (_ dismiss: @escaping () -> Void) -> SheetBody

    @State private var shown = false
    @State private var dragY: CGFloat = 0

    private let spring = Animation.spring(response: 0.34, dampingFraction: 0.9)

    /// Animate the card out, then clear the caller's binding.
    private func close() {
        withAnimation(spring) { shown = false; dragY = 0 }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.30) { requestClose() }
    }

    var body: some View {
        GeometryReader { geo in
            let maxH = geo.size.height * (size == .large ? 0.94 : 0.88)
            ZStack(alignment: .bottom) {
                // Scrim — tap to dismiss.
                Color.black.opacity(shown ? 0.45 : 0)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { close() }

                // Card.
                VStack(spacing: 0) {
                    grabber
                    sheetBody { close() }
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                }
                .frame(maxWidth: 720)
                .frame(maxHeight: maxH, alignment: .top)
                .frame(maxWidth: .infinity) // center horizontally on wide windows
                .background(theme.colors.bg)
                .clipShape(TopRoundedRectangle(radius: 22))
                .overlay(
                    TopRoundedRectangle(radius: 22)
                        .strokeBorder(theme.colors.border, lineWidth: 1)
                )
                .shadow(color: .black.opacity(shown ? 0.22 : 0), radius: 24, y: -6)
                .offset(y: shown ? dragY : maxH + 60)
                .gesture(dragToDismiss(maxH: maxH))
                .ignoresSafeArea(.container, edges: .bottom)
            }
        }
        .onAppear { withAnimation(spring) { shown = true } }
        #if os(iOS)
        .ignoresSafeArea(.keyboard)
        #endif
    }

    private var grabber: some View {
        Capsule()
            .fill(theme.colors.border)
            .frame(width: 38, height: 5)
            .padding(.top, 8)
            .padding(.bottom, 4)
            .frame(maxWidth: .infinity)
            .background(theme.colors.bg) // keep the handle area opaque + draggable
            .contentShape(Rectangle())
            .gesture(dragToDismiss(maxH: 600))
    }

    /// Drag the card down; release past a threshold (or with downward flick) closes.
    private func dragToDismiss(maxH: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 6)
            .onChanged { v in
                if v.translation.height > 0 { dragY = v.translation.height }
            }
            .onEnded { v in
                let flung = v.predictedEndTranslation.height > 220
                if dragY > maxH * 0.28 || flung {
                    close()
                } else {
                    withAnimation(spring) { dragY = 0 }
                }
            }
    }
}

/// A rectangle with only its top two corners rounded — version-safe (no
/// UnevenRoundedRectangle, which needs macOS 13.3 / iOS 16.4).
struct TopRoundedRectangle: InsettableShape {
    var radius: CGFloat
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = min(radius, min(rect.width, rect.height) / 2)
        let rr = rect.insetBy(dx: inset, dy: inset)
        var p = Path()
        p.move(to: CGPoint(x: rr.minX, y: rr.maxY))
        p.addLine(to: CGPoint(x: rr.minX, y: rr.minY + r))
        p.addArc(center: CGPoint(x: rr.minX + r, y: rr.minY + r),
                 radius: r, startAngle: .degrees(180), endAngle: .degrees(270), clockwise: false)
        p.addLine(to: CGPoint(x: rr.maxX - r, y: rr.minY))
        p.addArc(center: CGPoint(x: rr.maxX - r, y: rr.minY + r),
                 radius: r, startAngle: .degrees(270), endAngle: .degrees(0), clockwise: false)
        p.addLine(to: CGPoint(x: rr.maxX, y: rr.maxY))
        p.closeSubpath()
        return p
    }

    func inset(by amount: CGFloat) -> TopRoundedRectangle {
        TopRoundedRectangle(radius: radius, inset: inset + amount)
    }
}
