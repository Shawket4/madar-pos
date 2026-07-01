// Held orders (drafts) — parked carts the teller can restore later. Reached from
// the side nav rail / More drawer. Tapping a draft restores it into the cart
// (replacing the current one) and closes the sheet; the trash button discards it.
// All state + rules live in the core (cart::hold/restore_draft). Mirror of the
// Compose DraftsScreen.
import SwiftUI

struct DraftsView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            DraftsHeader(onClose: onClose)
            if app.drafts.isEmpty {
                VStack(spacing: Space.md) {
                    MadarIcon("tray", size: 36)
                        .foregroundStyle(theme.colors.textMuted)
                    Text(t("drafts.empty"))
                        .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: Space.md) {
                        ForEach(app.drafts, id: \.id) { d in
                            DraftCard(
                                draft: d, currency: currency,
                                onRestore: { restore(d) },
                                onDiscard: { app.discardDraft(d.id) }
                            )
                            .frame(maxWidth: 560)
                            .frame(maxWidth: .infinity)
                        }
                    }
                    .padding(Space.lg)
                }
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .onAppear { app.loadDrafts() }
    }

    private func restore(_ d: DraftView) {
        Haptics.impact()
        app.restoreDraft(d.id)
        onClose()
    }
}

/// Confident board header — back chevron, a leading teal tone-tile behind the tray
/// glyph, and the bold title on a surface bar with a hairline (mirrors Kitchen).
private struct DraftsHeader: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: Space.sm) {
            Button { onClose() } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            MadarIcon("tray.full", size: 18)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 34, height: 34)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            Text(t("drafts.title"))
                .font(.ui(20, .black)).foregroundStyle(theme.colors.textPrimary)
            Spacer()
        }
        .padding(.horizontal, Space.lg).padding(.vertical, 14)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

/// A parked-cart card — leading teal tray tile, name + item count, bold teal money
/// (the hero figure), and a danger discard tile. The whole card restores the draft.
private struct DraftCard: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let draft: DraftView
    let currency: String
    let onRestore: () -> Void
    let onDiscard: () -> Void

    var body: some View {
        Button(action: onRestore) {
            HStack(spacing: Space.md) {
                MadarIcon("tray.full", size: 20)
                    .foregroundStyle(theme.colors.accent)
                    .frame(width: 44, height: 44)
                    .background(theme.colors.accentBg)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                VStack(alignment: .leading, spacing: 3) {
                    Text(draft.name)
                        .font(.ui(16, .bold)).foregroundStyle(theme.colors.textPrimary)
                    Text("\(draft.itemCount) \(t("chrome.orders"))")
                        .font(.ui(12, .medium)).foregroundStyle(theme.colors.textMuted)
                }
                Spacer(minLength: Space.sm)
                // Money is the hero — heavy teal.
                Text(Money.format(draft.totalMinor, currency))
                    .font(.money(18, .black)).foregroundStyle(theme.colors.accent)
                Button(action: discard) {
                    MadarIcon("trash", size: 16)
                        .foregroundStyle(theme.colors.danger)
                        .frame(width: 40, height: 40)
                        .background(theme.colors.dangerBg)
                        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, Space.md)
            .padding(.vertical, 14)
            .background(theme.colors.surface)
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1))
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.99))
    }

    private func discard() {
        Haptics.selection()
        onDiscard()
    }
}
