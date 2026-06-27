// Void a synced order — pick a reason (+ optional note) and confirm. The void
// queues through the outbox (works offline); history flips to Voided immediately.
import SwiftUI

// `.sheet(item:)` needs Identifiable; the record already carries an `id`.
extension OrderSummaryView: Identifiable {}

struct VoidSheet: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let order: OrderSummaryView
    let onDone: () -> Void

    @State private var reason = "mistake"
    @State private var note = ""
    @State private var restoreInventory = true

    private var currency: String { app.session?.currencyCode ?? "" }
    private let reasons: [(key: String, label: String)] = [
        ("mistake", "void.reason_mistake"),
        ("customer", "void.reason_customer"),
        ("quality", "void.reason_quality"),
        ("other", "void.reason_other"),
    ]

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            ScrollView {
                VStack(spacing: Space.xl) {
                    HStack {
                        Text(t("void.title")).font(.ui(22, .heavy)).foregroundStyle(theme.colors.textPrimary)
                        Spacer()
                        Button { onDone() } label: {
                            MadarIcon("xmark", size: 16)
                                .foregroundStyle(theme.colors.textMuted)
                        }
                        .buttonStyle(.plain)
                    }

                    HStack {
                        Text(order.orderNumber.map { "#\($0)" } ?? t("history.order"))
                            .font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                        Spacer()
                        Text(Money.format(order.totalMinor, currency))
                            .font(.money(15, .bold)).foregroundStyle(theme.colors.textPrimary)
                    }
                    .padding(Space.md)
                    .background(theme.colors.surface)
                    .overlay(
                        RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                            .strokeBorder(theme.colors.borderLight, lineWidth: 1)
                    )
                    .elevation(.card)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))

                    VStack(alignment: .leading, spacing: Space.sm) {
                        Text(t("void.reason"))
                            .font(.ui(12, .bold)).tracking(0.6).foregroundStyle(theme.colors.textMuted)
                        ForEach(reasons, id: \.key) { r in reasonRow(r.key, t(r.label)) }
                    }

                    MadarTextField(placeholder: t("void.note"), text: $note, icon: "note.text", disabled: app.isBusy)

                    Divider().background(theme.colors.border)

                    HStack(spacing: Space.md) {
                        Text(t("void.restock"))
                            .font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                        Spacer(minLength: 0)
                        Toggle("", isOn: $restoreInventory)
                            .labelsHidden()
                            .tint(theme.colors.accent)
                    }

                    if let error = app.errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    }

                    HStack(spacing: Space.md) {
                        MadarButton(label: t("void.cancel"), variant: .outline) { onDone() }
                        MadarButton(label: t("void.confirm"), icon: "trash", variant: .danger, loading: app.isBusy) {
                            Task {
                                if await app.voidOrder(orderId: order.id, reason: reason, note: note, restoreInventory: restoreInventory) { onDone() }
                            }
                        }
                    }
                }
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
                .padding(Space.xl)
            }
        }
    }

    private func reasonRow(_ key: String, _ label: String) -> some View {
        let active = reason == key
        return Button {
            Haptics.selection()
            reason = key
        } label: {
            HStack(spacing: Space.md) {
                MadarIcon(active ? "largecircle.fill.circle" : "circle", size: IconSize.lg)
                    .foregroundStyle(active ? theme.colors.danger : theme.colors.textMuted)
                Text(label).font(.ui(14, active ? .bold : .medium)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
            }
            .padding(.vertical, Space.md)
            .padding(.horizontal, Space.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(active ? theme.colors.dangerBg : theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(active ? theme.colors.danger.opacity(0.45) : theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.99))
    }
}
