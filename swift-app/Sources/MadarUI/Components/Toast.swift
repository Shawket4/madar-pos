// A transient, app-wide toast/snackbar — a brief message that slides up from the
// bottom, optionally with one action (e.g. "Undo"), and auto-dismisses. Driven
// by `AppModel.toast`; rendered by `.toastHost(app)` on the root AND on modal
// sheets (only the topmost presentation is visible, so a toast raised from a
// sheet appears above it). Closures aren't Equatable, so the action lives on the
// model and the banner just signals a tap.
import SwiftUI

/// The data half of a toast (Equatable for clean transitions); the action lives
/// on `AppModel`.
struct ToastData: Equatable {
    let id: Int
    let text: String
    var icon: String?
    var tone: ChipTone = .neutral
    var actionLabel: String?
}

private struct ToastBanner: View {
    @Environment(\.theme) private var theme
    let data: ToastData
    let onAction: () -> Void

    private var accent: Color {
        switch data.tone {
        case .info: return theme.colors.navy
        case .accent: return theme.colors.accent
        case .success: return theme.colors.success
        case .warning: return theme.colors.warning
        case .danger: return theme.colors.danger
        case .neutral: return theme.colors.textSecondary
        }
    }

    var body: some View {
        HStack(spacing: Space.sm) {
            if let icon = data.icon {
                MadarIcon(icon, size: IconSize.sm).foregroundStyle(accent)
            }
            Text(data.text)
                .font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
            if let label = data.actionLabel {
                Spacer(minLength: Space.sm)
                Button(action: onAction) {
                    Text(label).font(.ui(13, .heavy)).foregroundStyle(accent)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surfaceRaised)
        .clipShape(Capsule())
        .overlay(Capsule().strokeBorder(theme.colors.border, lineWidth: 1))
        .shadow(color: .black.opacity(0.18), radius: 16, y: 6)
        .padding(.horizontal, Space.lg)
    }
}

private struct ToastHost: ViewModifier {
    @ObservedObject var app: AppModel

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if let toast = app.toast {
                ToastBanner(data: toast) { app.runToastAction() }
                    .padding(.bottom, Space.xxl)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .id(toast.id)
            }
        }
        // The toast layer never steals touches outside the banner.
        .allowsHitTesting(true)
    }
}

extension View {
    /// Render `app.toast` as a bottom banner over this view. Apply at the root and
    /// on modal sheets so toasts raised from either context are visible.
    func toastHost(_ app: AppModel) -> some View { modifier(ToastHost(app: app)) }
}
