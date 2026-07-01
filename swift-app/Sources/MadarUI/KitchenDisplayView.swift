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

    private var stationName: String {
        app.kdsStations.first(where: { $0.id == stationId })?.name ?? t("kds.title")
    }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                KitchenHeader(
                    stationName: stationName,
                    ticketCount: app.kdsTickets.count,
                    connected: app.realtimeConnected,
                    onSettings: { app.showSettings = true }
                )
                .madarSheet(isPresented: $app.showSettings, maxWidth: 560) { dismiss in
                    SettingsView(app: app, onClose: dismiss)
                }
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

    @ViewBuilder private var content: some View {
        if app.kdsTickets.isEmpty {
            KdsEmptyState()
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

// MARK: - Header

/// Clean, confident board header: a leading teal tone-tile behind the station
/// glyph, the bold station name, a live-connection dot, and the outstanding count.
private struct KitchenHeader: View {
    let stationName: String
    let ticketCount: Int
    let connected: Bool
    let onSettings: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: Space.sm) {
            MadarIcon("fork.knife", size: 18)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 34, height: 34)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            Text(stationName).font(.ui(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Circle()
                .fill(connected ? theme.colors.success : theme.colors.textMuted)
                .frame(width: 8, height: 8)
            Spacer()
            if ticketCount > 0 {
                StatusChip(label: "\(ticketCount)", tone: .accent)
            }
            Button(action: onSettings) {
                MadarIcon("gearshape", size: 18).foregroundStyle(theme.colors.textSecondary)
            }
            .buttonStyle(.plain)
            .padding(.leading, Space.xs)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, 14)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

// MARK: - All-clear empty state

private struct KdsEmptyState: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        VStack(spacing: Space.md) {
            Spacer()
            MadarIcon("checkmark.circle", size: 36)
                .foregroundStyle(theme.colors.success)
                .frame(width: 72, height: 72)
                .background(theme.colors.successBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous))
            Text(t("kds.all_clear"))
                .font(.ui(16, .semibold)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Ticket card

/// A raised white card matching the catalog/cart cards: soft hairline + card
/// elevation, Radii.md corners. The age SLA is the hero — a tinted tone badge that
/// escalates fresh accent → amber (5m) → red (10m); a ready ticket is success and
/// gets a heavier border so it pops on the board.
private struct KdsTicketCard: View {
    @ObservedObject var app: AppModel
    let ticket: KdsTicketView
    @Environment(\.theme) private var theme

    private var isReady: Bool { ticket.status == "ready" }
    private var ageMinutes: Int {
        guard let date = ISO8601DateFormatter().date(from: ticket.createdAt) else { return 0 }
        return max(0, Int(Date().timeIntervalSince(date) / 60))
    }
    /// Age coloring: fresh accent → amber (5m) → red (10m); ready is always success.
    private var ageFg: Color {
        if isReady { return theme.colors.success }
        if ageMinutes >= 10 { return theme.colors.danger }
        if ageMinutes >= 5 { return theme.colors.warning }
        return theme.colors.accent
    }
    private var ageBg: Color {
        if isReady { return theme.colors.successBg }
        if ageMinutes >= 10 { return theme.colors.dangerBg }
        if ageMinutes >= 5 { return theme.colors.warningBg }
        return theme.colors.accentBg
    }

    private func bump(_ line: KdsLineView) {
        Task {
            if line.bumped { await app.unbumpKdsItem(line.id) }
            else { await app.bumpKdsItem(line.id) }
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            // Age-tinted header strip — fixed height aligns every card's first item.
            HStack(spacing: Space.sm) {
                Text(ticket.tableLabel ?? ticket.kitchenRef ?? "#\(ticket.roundNumber)")
                    .font(.ui(19, .heavy)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                if ticket.sourceType == "open_ticket" {
                    MadarIcon("person.fill", size: 16).foregroundStyle(ageFg)
                }
                Spacer()
                Text("\(ageMinutes)m").font(.money(18, .heavy)).foregroundStyle(ageFg)
            }
            .padding(.horizontal, Space.md)
            .frame(height: 54)
            .frame(maxWidth: .infinity)
            .background(ageBg)
            VStack(alignment: .leading, spacing: 0) {
                ForEach(ticket.items, id: \.id) { line in
                    KdsLineRow(line: line) { bump(line) }
                }
            }
            .padding(.horizontal, Space.md).padding(.vertical, Space.xs)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(isReady ? ageFg.opacity(0.6) : theme.colors.borderLight,
                              lineWidth: isReady ? 2 : 1)
        )
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

// MARK: - Bumpable line

/// One tappable line: a check toggle, qty × name (+ size), modifiers, and an
/// optional kitchen note (warning-tinted). Bumped lines mute + strike through. The
/// per-line station label (expo board) pins to the trailing edge.
private struct KdsLineRow: View {
    let line: KdsLineView
    let onBump: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        Button(action: onBump) {
            HStack(alignment: .top, spacing: Space.sm) {
                MadarIcon(line.bumped ? "checkmark.circle.fill" : "circle", size: 22)
                    .foregroundStyle(line.bumped ? theme.colors.success : theme.colors.textMuted)
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(line.qty)× \(line.name)\(line.sizeLabel.map { " · \($0)" } ?? "")")
                        .font(.ui(15, .semibold))
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
                // Per-line station label (expo / all-station board) — the core
                // populates stationName but it was never rendered before.
                if let station = line.stationName, !station.isEmpty {
                    Text(station.uppercased()).font(.ui(10, .bold))
                        .foregroundStyle(theme.colors.textMuted)
                }
            }
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.pressable)
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
