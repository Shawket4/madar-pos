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

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                SyncHeader(app: app, onClose: onClose)
                if app.outbox.isEmpty {
                    SyncEmptyState()
                } else {
                    outboxList
                }
            }
        }
        .task { app.loadOutbox() }
    }

    private var outboxList: some View {
        ScrollView {
            // One surface card; rows separated by hairlines — matches Flutter's
            // `_EntryGroupCard` (single `SurfaceCard(padding: zero)` with `Divider`
            // between rows), not per-row cards.
            VStack(spacing: 0) {
                ForEach(Array(app.outbox.enumerated()), id: \.element.id) { index, item in
                    if index > 0 { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
                    SyncRow(app: app, item: item)
                }
            }
            .background(theme.colors.surface)
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1))
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
            .padding(Space.lg)
        }
    }
}

// MARK: - Header

/// Clean bold title with the back affordance, plus the two queue actions (Retry
/// the failed rows, force-push everything queued).
private struct SyncHeader: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    // Retry requeues only the FAILED (dead) rows, so it only appears when there's
    // something dead to resurrect.
    private var hasFailed: Bool { app.outbox.contains { $0.status == "dead" } }

    var body: some View {
        HStack(spacing: Space.sm) {
            Button { onClose() } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            // Leading teal tone-tile behind the sync glyph — matches the confident
            // Kitchen/Order header (accentBg + accent icon, 34×34, Radii.sm).
            MadarIcon("arrow.triangle.2.circlepath", size: IconSize.lg)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 34, height: 34)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            Text(t("sync.title")).font(.ui(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer(minLength: 0)
            HStack(spacing: Space.lg) {
                if hasFailed {
                    SyncRetryButton { Task { await app.retryOutbox() } }
                }
                // "Sync now" force-pushes every QUEUED command (not just dead ones) —
                // the manual escape hatch when the queue isn't draining on its own.
                // Visible whenever anything is waiting to sync.
                if !app.outbox.isEmpty {
                    SyncNowButton(pushing: app.isPushing) { Task { await app.syncNow() } }
                }
            }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, 14)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

/// Requeue-the-failed action — a quiet accent text button.
private struct SyncRetryButton: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onTap: () -> Void

    private func tap() {
        Haptics.selection()
        onTap()
    }

    var body: some View {
        Button(action: tap) {
            HStack(spacing: 6) {
                MadarIcon("arrow.clockwise", size: IconSize.sm)
                Text(t("sync.retry"))
            }
            .font(.ui(13, .semibold)).foregroundStyle(theme.colors.accent)
        }
        .buttonStyle(.pressable)
    }
}

/// Force-push-the-queue action — the teal pill CTA; spins + disables while pushing.
private struct SyncNowButton: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let pushing: Bool
    let onTap: () -> Void

    private func tap() {
        Haptics.selection()
        onTap()
    }

    var body: some View {
        Button(action: tap) {
            HStack(spacing: 6) {
                if pushing {
                    ProgressView().controlSize(.small).tint(theme.colors.textOnAccent)
                } else {
                    MadarIcon("icloud.and.arrow.up", size: IconSize.sm)
                }
                Text(pushing ? t("sync.pushing") : t("sync.push"))
            }
            .font(.ui(13, .heavy)).foregroundStyle(theme.colors.textOnAccent)
            .padding(.horizontal, Space.md)
            .padding(.vertical, 7)
            .background(theme.colors.accent)
            .clipShape(Capsule())
        }
        .buttonStyle(.pressable)
        .disabled(pushing)
    }
}

// MARK: - Empty state

/// Nothing waiting to sync — a reassuring success mark.
private struct SyncEmptyState: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        VStack(spacing: Space.md) {
            MadarIcon("checkmark.circle", size: 36)
                .foregroundStyle(theme.colors.success)
                .frame(width: 72, height: 72)
                .background(theme.colors.successBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous))
            Text(t("sync.empty")).font(.ui(16, .semibold)).foregroundStyle(theme.colors.textSecondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Outbox row

private struct SyncRow: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let item: OutboxItemView

    // Outbox tones map to the shared ChipTone scale: failed → .danger, everything
    // else (queued / in-flight) → .info. The leading tile tint reuses tone.bg/fg,
    // so the icon and the status chip always read as the same tone.
    private var tone: ChipTone { item.status == "dead" ? .danger : .info }

    private func discard() {
        Haptics.selection()
        app.discardOutboxItem(item.id)
    }

    var body: some View {
        HStack(spacing: Space.md) {
            // Leading op-type tile — 40×40, Radii.sm, tone-tinted bg + op glyph.
            ZStack {
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .fill(tone.bg(theme.colors))
                    .frame(width: 40, height: 40)
                MadarIcon(opIcon, size: IconSize.lg)
                    .foregroundStyle(tone.fg(theme.colors))
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(opLabel).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textPrimary)
                if let err = item.lastError, !err.isEmpty {
                    Text(err).font(.ui(12)).foregroundStyle(theme.colors.textMuted).lineLimit(2)
                } else if item.attempts > 0 {
                    Text("\(item.attempts) \(t("sync.attempts"))")
                        .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                }
            }
            Spacer(minLength: Space.sm)
            StatusChip(label: statusLabel, tone: tone)
            if item.status == "dead" {
                Button(action: discard) {
                    MadarIcon("trash", size: IconSize.md)
                        .foregroundStyle(theme.colors.danger)
                        .padding(Space.xs)
                        .contentShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
                }
                .buttonStyle(.pressable)
            }
        }
        // Fixed min row height keeps icon tiles on a consistent vertical rhythm
        // whether or not a row carries a subtitle.
        .frame(minHeight: 68)
        .padding(.leading, Space.lg)
        .padding(.trailing, Space.md)
        .padding(.vertical, Space.md)
    }

    // Op glyph — dead → warning mark, else per op_type.
    private var opIcon: String {
        if item.status == "dead" { return "exclamationmark.circle" }
        switch item.opType {
        case "open_shift": return "play.circle"
        case "close_shift": return "lock"
        case "create_order": return "doc.text"
        default: return "arrow.triangle.2.circlepath"
        }
    }

    private var opLabel: String {
        switch item.opType {
        case "open_shift": return t("sync.op_open_shift")
        case "close_shift": return t("sync.op_close_shift")
        case "create_order": return t("sync.op_create_order")
        default: return item.opType
        }
    }

    private var statusLabel: String {
        switch item.status {
        case "dead": return t("sync.failed")
        case "inflight": return t("sync.sending")
        default: return t("sync.queued")
        }
    }
}
