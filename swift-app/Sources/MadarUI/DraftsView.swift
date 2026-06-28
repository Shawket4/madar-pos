// Held orders (drafts) — parked carts the teller can restore later. Reached from
// the More drawer. Tapping a draft restores it into the cart (replacing the
// current one) and closes the sheet; the trash button discards it. All state +
// rules live in the core (cart::hold/restore_draft). Mirror of the Flutter drafts.
import SwiftUI

struct DraftsView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            header
            if app.drafts.isEmpty {
                VStack(spacing: Space.md) {
                    MadarIcon("tray", size: 36)
                        .foregroundStyle(theme.colors.textMuted)
                    Text(t("drafts.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: Space.md) {
                        ForEach(app.drafts, id: \.id) { draftRow($0) }
                    }
                    .frame(maxWidth: 560).frame(maxWidth: .infinity).padding(Space.lg)
                }
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .onAppear { app.loadDrafts() }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            Text(t("drafts.title")).font(.ui(17, .bold)).foregroundStyle(theme.colors.textPrimary)
            Spacer()
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func draftRow(_ d: DraftView) -> some View {
        Button {
            Haptics.impact()
            app.restoreDraft(d.id)
            onClose()
        } label: {
            HStack(spacing: Space.md) {
                MadarIcon("tray.full", size: 17)
                    .foregroundStyle(theme.colors.accent)
                    .frame(width: 34, height: 34)
                    .background(theme.colors.surfaceAlt)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
                VStack(alignment: .leading, spacing: 2) {
                    Text(d.name).font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    Text("\(d.itemCount) \(t("chrome.orders"))")
                        .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                }
                Spacer(minLength: Space.sm)
                Text(Money.format(d.totalMinor, currency))
                    .font(.money(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                Button {
                    Haptics.selection()
                    app.discardDraft(d.id)
                } label: {
                    MadarIcon("trash", size: 14)
                        .foregroundStyle(theme.colors.danger)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, Space.md)
            .padding(.vertical, Space.sm + 2)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.99))
    }
}
