// In-app realtime alert — the visual companion to the OS notification (the
// rebuild of the Flutter `NewOrderBanner`, generalized to every alerting event
// and made more polished). Driven by the core's dedup'd alert
// (RealtimeAlertPlayer.postNotification → AppModel.showRealtimeBanner). A spring-in
// accent banner with a looping bounce+wiggle icon over a pulsing glow ring;
// auto-dismisses, or tap / ✕ to clear.
import SwiftUI

/// One in-app alert. `tag` is the core's `"{event_type}:{id}"`, so its prefix
/// picks the icon (delivery / kitchen / ticket / ready).
struct RealtimeAlert: Identifiable, Equatable {
    let id: Int
    let title: String
    let body: String
    let tag: String

    var icon: String {
        let type = tag.split(separator: ":").first.map(String.init) ?? ""
        if type.contains("ready") { return "bell.fill" }
        if type.hasPrefix("delivery") { return "bicycle" }
        if type.hasPrefix("kitchen") { return "flame.fill" }
        if type.hasPrefix("ticket") { return "fork.knife" }
        return "bell.fill"
    }
}

extension View {
    /// Mount the top-anchored realtime alert banner over the app root.
    func realtimeAlertHost(_ app: AppModel) -> some View { modifier(RealtimeAlertHostModifier(app: app)) }
}

private struct RealtimeAlertHostModifier: ViewModifier {
    @ObservedObject var app: AppModel
    func body(content: Content) -> some View {
        content.overlay(alignment: .top) {
            if let alert = app.realtimeAlert {
                RealtimeAlertCard(alert: alert) { app.dismissRealtimeAlert(alert.id) }
                    .padding(.horizontal, Space.md)
                    .padding(.top, Space.sm)
                    .frame(maxWidth: 560)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .zIndex(40)
            }
        }
        .animation(.spring(response: 0.34, dampingFraction: 0.9), value: app.realtimeAlert)
    }
}

struct RealtimeAlertCard: View {
    let alert: RealtimeAlert
    let onDismiss: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: Space.md) {
            // Looping bounce + wiggle (the Flutter LoopingIcon, ported): the glyph
            // pulses on |sin| and wiggles on a faster sin, over a pulsing glow ring.
            TimelineView(.animation) { tl in
                let p = tl.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 0.9) / 0.9
                let scale = 1 + 0.18 * abs(sin(p * 2 * .pi))
                let angle = 0.18 * sin(p * 4 * .pi)
                ZStack {
                    Circle().fill(theme.colors.accent.opacity(Opacity.subtle))
                        .frame(width: 34, height: 34)
                        .scaleEffect(scale)
                    MadarIcon(alert.icon, size: 20).foregroundStyle(theme.colors.accent)
                        .scaleEffect(scale)
                        .rotationEffect(.radians(angle))
                }
            }
            .frame(width: 34, height: 34)

            VStack(alignment: .leading, spacing: 1) {
                Text(alert.title).font(.ui(13, .heavy)).foregroundStyle(theme.colors.accent).lineLimit(1)
                if !alert.body.isEmpty {
                    Text(alert.body).font(.ui(11)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
                }
            }
            Spacer(minLength: Space.sm)
            Button(action: onDismiss) {
                MadarIcon("xmark", size: 16).foregroundStyle(theme.colors.textMuted)
                    .frame(width: 28, height: 28)
            }.buttonStyle(.plain)
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(theme.colors.accentBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.accent.opacity(0.35), lineWidth: 1)
        )
        .elevation(.raised)
        .contentShape(Rectangle())
        .onTapGesture { Haptics.selection(); onDismiss() }
    }
}
