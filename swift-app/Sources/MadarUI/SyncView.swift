// Sync center — visibility into the durable outbox: what's queued, what's in
// flight, and what failed (with the error). Retry requeues every failed command;
// a teller can discard a dead one they've given up on. Reachable from the order
// action bar. Everything reads locally, so it works offline.
import SwiftUI

struct SyncView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    private var hasFailed: Bool { app.outbox.contains { $0.status == "dead" } }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                if app.outbox.isEmpty {
                    VStack(spacing: Space.md) {
                        MadarIcon("checkmark.circle", size: 40)
                            .foregroundStyle(theme.colors.success)
                        Text(t("sync.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        // One surface card; rows separated by hairlines — matches
                        // Flutter's `_EntryGroupCard` (single `SurfaceCard(padding:
                        // zero)` with `Divider` between rows), not per-row cards.
                        VStack(spacing: 0) {
                            ForEach(Array(app.outbox.enumerated()), id: \.element.id) { index, item in
                                if index > 0 { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
                                row(item)
                            }
                        }
                        .background(theme.colors.surface)
                        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                            .strokeBorder(theme.colors.border, lineWidth: 1))
                        .frame(maxWidth: 560)
                        .frame(maxWidth: .infinity)
                        .padding(Space.lg)
                    }
                }
            }
        }
        .task { app.loadOutbox() }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            Text(t("sync.title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer(minLength: 0)
            if hasFailed {
                Button {
                    Haptics.selection()
                    Task { await app.retryOutbox() }
                } label: {
                    HStack(spacing: 6) {
                        MadarIcon("arrow.clockwise", size: IconSize.sm)
                        Text(t("sync.retry"))
                    }
                    .font(.ui(13, .semibold)).foregroundStyle(theme.colors.accent)
                }
                .buttonStyle(.pressable)
            }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func row(_ item: OutboxItemView) -> some View {
        let tone: ChipTone = statusTone(item.status)
        return HStack(spacing: Space.md) {
            // Leading tone icon — Flutter `_EntryRow`: 38×38, Radii.xs, tone bg.
            ZStack {
                RoundedRectangle(cornerRadius: Radii.xs, style: .continuous)
                    .fill(tone.bg(theme.colors))
                    .frame(width: 38, height: 38)
                MadarIcon(opIcon(item.opType, status: item.status), size: IconSize.lg)
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(tone.fg(theme.colors))
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(opLabel(item.opType)).font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                if let err = item.lastError, !err.isEmpty {
                    Text(err).font(.ui(11)).foregroundStyle(theme.colors.textMuted).lineLimit(2)
                } else if item.attempts > 0 {
                    Text("\(item.attempts) \(t("sync.attempts"))")
                        .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                }
            }
            Spacer(minLength: Space.sm)
            StatusChip(label: statusLabel(item.status), tone: tone)
            if item.status == "dead" {
                Button {
                    Haptics.selection()
                    app.discardOutboxItem(item.id)
                } label: {
                    MadarIcon("trash", size: 14)
                        .foregroundStyle(theme.colors.danger)
                }
                .buttonStyle(.pressable)
            }
        }
        // Flutter row padding: fromSTEB(lg, md, md, md).
        .padding(.leading, Space.lg)
        .padding(.trailing, Space.md)
        .padding(.vertical, Space.md)
    }

    private func opIcon(_ op: String, status: String) -> String {
        if status == "dead" { return "exclamationmark.circle" }
        switch op {
        case "open_shift": return "play.circle"
        case "close_shift": return "lock"
        case "create_order": return "doc.text"
        default: return "arrow.triangle.2.circlepath"
        }
    }

    private func opLabel(_ op: String) -> String {
        switch op {
        case "open_shift": return t("sync.op_open_shift")
        case "close_shift": return t("sync.op_close_shift")
        case "create_order": return t("sync.op_create_order")
        default: return op
        }
    }
    private func statusLabel(_ s: String) -> String {
        switch s {
        case "dead": return t("sync.failed")
        case "inflight": return t("sync.sending")
        default: return t("sync.queued")
        }
    }
    private func statusTone(_ s: String) -> ChipTone { s == "dead" ? .danger : .info }
}
