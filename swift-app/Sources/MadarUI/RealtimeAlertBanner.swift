// In-app realtime alert — the visual companion to the OS notification (the
// rebuild of the Flutter `NewOrderBanner`, generalized to every alerting event).
// Driven by the core's dedup'd alert (RealtimeAlertPlayer.postNotification →
// AppModel.showRealtimeBanner). PERSISTENT (stays until dismissed) and rendered
// like the iOS notification stack: a compact collapsed DECK (top card full, the
// rest peeking behind) that expands to a scrollable list on tap — so it overlays
// the app, never pushes content down, and stays small no matter how many pile in.
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
    /// Mount the top-anchored realtime alert stack over the app root.
    func realtimeAlertHost(_ app: AppModel) -> some View { modifier(RealtimeAlertHostModifier(app: app)) }
}

private struct StackHeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct RealtimeAlertHostModifier: ViewModifier {
    @ObservedObject var app: AppModel
    @State private var expanded = false
    @State private var contentHeight: CGFloat = 0

    func body(content: Content) -> some View {
        content.overlay(alignment: .top) {
            if !app.realtimeAlerts.isEmpty {
                Group {
                    if expanded { expandedList } else { collapsedDeck }
                }
                .frame(maxWidth: 480)
                .padding(.horizontal, Space.md)
                .padding(.top, Space.sm)
                .transition(.move(edge: .top).combined(with: .opacity))
                .zIndex(40)
            }
        }
        .animation(.spring(response: 0.34, dampingFraction: 0.9), value: app.realtimeAlerts)
        .animation(.spring(response: 0.34, dampingFraction: 0.9), value: expanded)
        .onChange(of: app.realtimeAlerts.count) { count in if count <= 1 { expanded = false } }
    }

    /// iOS-style collapsed deck: newest on top at full size; up to two behind peek
    /// out — scaled down, nudged down, dimmed. Tap to fan it out.
    private var collapsedDeck: some View {
        let deck = Array(app.realtimeAlerts.prefix(3).enumerated())
        return ZStack(alignment: .top) {
            // Render deepest first so the newest (depth 0) lands on top.
            ForEach(deck.reversed(), id: \.element.id) { (depth, alert) in
                RealtimeAlertCard(
                    alert: alert,
                    animated: depth == 0,
                    showClose: depth == 0,
                    onTap: app.realtimeAlerts.count > 1 ? { expanded = true } : nil,
                    onDismiss: { app.dismissRealtimeAlert(alert.id) }
                )
                .scaleEffect(1 - 0.05 * CGFloat(depth), anchor: .top)
                .offset(y: 10 * CGFloat(depth))
                .opacity(1 - 0.22 * Double(depth))
                .zIndex(Double(3 - depth))
            }
        }
    }

    /// Fanned-out, scrollable list (newest on top) + a chevron-up to re-collapse.
    /// Sized to content up to a cap, then scrolls — it never pushes the app down.
    private var expandedList: some View {
        ScrollView {
            VStack(spacing: Space.sm) {
                ForEach(app.realtimeAlerts) { alert in
                    RealtimeAlertCard(
                        alert: alert, animated: true, showClose: true, onTap: nil,
                        onDismiss: { app.dismissRealtimeAlert(alert.id) }
                    )
                    .transition(.move(edge: .top).combined(with: .opacity))
                }
                Button { expanded = false } label: {
                    MadarIcon("chevron.up", size: 16).foregroundStyle(Color.secondary)
                        .padding(.horizontal, Space.lg).padding(.vertical, 6)
                        .background(Capsule().fill(.thinMaterial))
                }.buttonStyle(.plain)
            }
            .background(GeometryReader { g in
                Color.clear.preference(key: StackHeightKey.self, value: g.size.height)
            })
        }
        .frame(height: min(contentHeight, 440))
        .onPreferenceChange(StackHeightKey.self) { contentHeight = $0 }
    }
}

struct RealtimeAlertCard: View {
    let alert: RealtimeAlert
    var animated: Bool = true
    var showClose: Bool = true
    var onTap: (() -> Void)? = nil
    let onDismiss: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: Space.md) {
            iconView.frame(width: 34, height: 34)
            VStack(alignment: .leading, spacing: 1) {
                Text(alert.title).font(.ui(13, .heavy)).foregroundStyle(theme.colors.accent).lineLimit(1)
                if !alert.body.isEmpty {
                    Text(alert.body).font(.ui(11)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
                }
            }
            Spacer(minLength: Space.sm)
            if showClose {
                Button(action: onDismiss) {
                    MadarIcon("xmark", size: 16).foregroundStyle(theme.colors.textMuted)
                        .frame(width: 28, height: 28)
                }.buttonStyle(.plain)
            }
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
        .onTapGesture { Haptics.selection(); (onTap ?? onDismiss)() }
    }

    // Looping bounce + wiggle (the Flutter LoopingIcon, ported); static for the
    // peeking deck cards.
    @ViewBuilder private var iconView: some View {
        if animated {
            TimelineView(.animation) { tl in
                let p = tl.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 0.9) / 0.9
                iconStack(scale: 1 + 0.18 * abs(sin(p * 2 * .pi)), angle: 0.18 * sin(p * 4 * .pi))
            }
        } else {
            iconStack(scale: 1, angle: 0)
        }
    }

    private func iconStack(scale: CGFloat, angle: Double) -> some View {
        ZStack {
            Circle().fill(theme.colors.accent.opacity(Opacity.subtle))
                .frame(width: 34, height: 34).scaleEffect(scale)
            MadarIcon(alert.icon, size: 20).foregroundStyle(theme.colors.accent)
                .scaleEffect(scale).rotationEffect(.radians(angle))
        }
    }
}
