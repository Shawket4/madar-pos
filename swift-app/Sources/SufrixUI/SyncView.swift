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
                        Image(systemName: "checkmark.circle")
                            .font(.system(size: 40, weight: .light))
                            .foregroundStyle(theme.colors.success)
                        Text(t("sync.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        VStack(spacing: Space.sm) {
                            ForEach(app.outbox, id: \.id) { row($0) }
                        }
                        .frame(maxWidth: 520)
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
                Image(systemName: "chevron.left").font(.system(size: 17, weight: .semibold))
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
                        Image(systemName: "arrow.clockwise")
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
        HStack(spacing: Space.md) {
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
            StatusChip(label: statusLabel(item.status), tone: statusTone(item.status))
            if item.status == "dead" {
                Button {
                    Haptics.selection()
                    app.discardOutboxItem(item.id)
                } label: {
                    Image(systemName: "trash").font(.system(size: 14))
                        .foregroundStyle(theme.colors.danger)
                }
                .buttonStyle(.pressable)
            }
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
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
