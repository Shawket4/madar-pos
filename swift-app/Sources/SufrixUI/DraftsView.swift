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
                    Image(systemName: "tray").font(.system(size: 36, weight: .light))
                        .foregroundStyle(theme.colors.textMuted)
                    Text(t("drafts.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(spacing: Space.sm) {
                        ForEach(app.drafts, id: \.id) { draftRow($0) }
                    }
                    .frame(maxWidth: 520).frame(maxWidth: .infinity).padding(Space.lg)
                }
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .onAppear { app.loadDrafts() }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                Image(systemName: "chevron.backward").font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.plain)
            Text(t("drafts.title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
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
                Image(systemName: "tray.full").font(.system(size: 18))
                    .foregroundStyle(theme.colors.accent).frame(width: 24)
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
                    Image(systemName: "trash").font(.system(size: 14))
                        .foregroundStyle(theme.colors.danger)
                }
                .buttonStyle(.plain)
            }
            .padding(Space.md)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.99))
    }
}
