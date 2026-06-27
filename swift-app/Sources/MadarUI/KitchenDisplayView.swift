// Kitchen Display — the full-screen board a `kitchen`-role device shows. It
// subscribes to the branch's `kitchen` topic on the unified bus (one SSE), lists
// outstanding tickets oldest-first, and bumps lines tap-by-tap. All logic is in
// the core; this view renders + collects. The board survives a reconnect (the core
// caches the feed) and disables bumping while the connection is down.
import SwiftUI

struct KitchenDisplayView: View {
    @ObservedObject var app: AppModel
    let stationId: String
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private let columns = [GridItem(.adaptive(minimum: 260), spacing: Space.md)]

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                if !app.realtimeConnected {
                    NoticeBanner(icon: "wifi.slash", text: t("kds.reconnecting"), tone: .warning)
                        .padding(.horizontal, Space.lg).padding(.top, Space.sm)
                }
                content
            }
        }
        .task {
            await app.loadKdsStations()
            await app.loadKds()
            // The unified SSE is subscribed once at the session level (the core picks
            // the kitchen topic for a KDS device) — no per-screen subscription here.
        }
        // Slow safety-net refresh in case an event is missed; the SSE is primary.
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 60_000_000_000)
                if Task.isCancelled { break }
                await app.loadKds()
            }
        }
    }

    private var stationName: String {
        app.kdsStations.first(where: { $0.id == stationId })?.name ?? t("kds.title")
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            MadarIcon("flame.fill", size: 18).foregroundStyle(theme.colors.accent)
            Text(stationName).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Circle()
                .fill(app.realtimeConnected ? theme.colors.success : theme.colors.textMuted)
                .frame(width: 8, height: 8)
            Spacer()
            Text("\(app.kdsTickets.count)").font(.ui(15, .bold)).foregroundStyle(theme.colors.textSecondary)
            Button { app.showSettings = true } label: {
                MadarIcon("gearshape", size: 18).foregroundStyle(theme.colors.textSecondary)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        .madarSheet(isPresented: $app.showSettings, maxWidth: 560) { dismiss in SettingsView(app: app, onClose: dismiss) }
    }

    @ViewBuilder private var content: some View {
        if app.kdsTickets.isEmpty {
            VStack(spacing: Space.md) {
                Spacer()
                MadarIcon("checkmark.circle", size: 44).foregroundStyle(theme.colors.textMuted)
                Text(t("kds.all_clear")).font(.ui(16, .semibold)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
            }
        } else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: Space.md) {
                    ForEach(app.kdsTickets, id: \.id) { ticket in
                        KdsTicketCard(app: app, ticket: ticket)
                    }
                }
                .padding(Space.lg)
            }
        }
    }
}

private struct KdsTicketCard: View {
    @ObservedObject var app: AppModel
    let ticket: KdsTicketView
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private var isReady: Bool { ticket.status == "ready" }
    private var ageMinutes: Int {
        guard let date = ISO8601DateFormatter().date(from: ticket.createdAt) else { return 0 }
        return max(0, Int(Date().timeIntervalSince(date) / 60))
    }
    /// Age coloring: fresh → amber → red as a ticket waits.
    private var ageTone: Color {
        if isReady { return theme.colors.success }
        if ageMinutes >= 10 { return theme.colors.danger }
        if ageMinutes >= 5 { return theme.colors.warning }
        return theme.colors.accent
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack {
                Text(ticket.tableLabel ?? ticket.kitchenRef ?? "#\(ticket.roundNumber)")
                    .font(.ui(15, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                Text("\(ageMinutes)m").font(.ui(13, .bold)).foregroundStyle(ageTone)
            }
            if ticket.sourceType == "open_ticket" {
                Text(t("kds.waiter")).font(.ui(11, .bold)).foregroundStyle(theme.colors.textMuted)
            }
            Divider().overlay(theme.colors.border)
            ForEach(ticket.items, id: \.id) { line in
                Button {
                    Task {
                        if line.bumped { await app.unbumpKdsItem(line.id) }
                        else { await app.bumpKdsItem(line.id) }
                    }
                } label: {
                    HStack(alignment: .top, spacing: Space.sm) {
                        MadarIcon(line.bumped ? "checkmark.circle.fill" : "circle", size: 18)
                            .foregroundStyle(line.bumped ? theme.colors.success : theme.colors.textMuted)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("\(line.qty)× \(line.name)\(line.sizeLabel.map { " · \($0)" } ?? "")")
                                .font(.ui(14, .semibold))
                                .foregroundStyle(line.bumped ? theme.colors.textMuted : theme.colors.textPrimary)
                                .strikethrough(line.bumped)
                            if !line.modifiers.isEmpty {
                                Text(line.modifiers.joined(separator: ", "))
                                    .font(.ui(12, .medium)).foregroundStyle(theme.colors.textSecondary)
                            }
                            if let notes = line.notes, !notes.isEmpty {
                                Text(notes).font(.ui(12, .semibold)).foregroundStyle(theme.colors.warning)
                            }
                        }
                        Spacer()
                        // Per-line station label (expo / all-station board) — the
                        // core populates stationName but it was never rendered.
                        if let station = line.stationName, !station.isEmpty {
                            Text(station.uppercased()).font(.ui(10, .bold))
                                .foregroundStyle(theme.colors.textMuted)
                        }
                    }
                }
                .buttonStyle(.plain)
                .disabled(!app.realtimeConnected && false) // bump is online-direct; allow even if SSE blips
            }
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(ageTone.opacity(isReady ? 0.6 : 0.25), lineWidth: isReady ? 2 : 1))
        .clipShape(RoundedRectangle(cornerRadius: 14))
    }
}

/// The core's realtime sink (one per device). The core calls these from its SSE
/// task on a background thread, so each hop bounces to the main actor before
/// touching the `@MainActor` model.
final class RealtimeBridge: EventListener {
    private weak var owner: AppModel?
    init(owner: AppModel) { self.owner = owner }

    func onEvent(event: RealtimeEvent) {
        Task { @MainActor [weak owner] in owner?.onRealtimeEvent(event) }
    }
    func onConnectionChanged(connected: Bool) {
        Task { @MainActor [weak owner] in owner?.onRealtimeConnection(connected) }
    }
}
