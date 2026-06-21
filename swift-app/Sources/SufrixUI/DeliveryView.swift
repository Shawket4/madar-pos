// Delivery queue — the teller works a branch's live delivery orders: advance the
// lifecycle (Confirm → Preparing → Ready → Out for delivery → Delivered), bump
// prep time, cancel (with restock), and finalize into a real sale on the open
// shift. All logic is in the core; this view only renders + collects. Online —
// it refreshes on appear and on a light poll while open.
import SwiftUI

struct DeliveryView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var finalizing: DeliveryOrderView?
    @State private var cancelling: DeliveryOrderView?

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                if let s = app.deliverySettings { acceptingBar(s) }
                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning).padding(Space.lg)
                }
                content
            }
        }
        .task { await app.loadDeliveryOrders() }
        // Light poll while the queue is open (the rebuild's realtime stand-in).
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 15_000_000_000)
                if Task.isCancelled { break }
                await app.loadDeliveryOrders()
            }
        }
        .sufrixSheet(item: $finalizing) { order, dismiss in
            FinalizeSheet(app: app, order: order, onClose: dismiss)
        }
        .sufrixSheet(item: $cancelling) { order, dismiss in
            CancelSheet(app: app, order: order, onClose: dismiss)
        }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                Image(systemName: "chevron.backward").font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.plain)
            Text(t("delivery.queue")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer()
            // Active-only ↔ all toggle.
            Picker("", selection: $app.deliveryActiveOnly) {
                Text(t("delivery.active")).tag(true)
                Text(t("delivery.all")).tag(false)
            }
            .pickerStyle(.segmented)
            .frame(width: 170)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    /// Per-channel accepting control — tap a channel to cycle auto → open → closed.
    private func acceptingBar(_ s: DeliverySettingsView) -> some View {
        HStack(spacing: Space.sm) {
            Text(t("delivery.accepting")).font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted)
            acceptingChip(t("delivery.in_mall"), channel: "in_mall", mode: s.inMallOverride, enabled: s.inMallEnabled)
            acceptingChip(t("delivery.outside"), channel: "outside", mode: s.outsideOverride, enabled: s.outsideEnabled)
            Spacer()
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.sm)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func acceptingChip(_ label: String, channel: String, mode: String, enabled: Bool) -> some View {
        // Dashboard-disabled channels can't be opened; show them muted.
        let tone: ChipTone = !enabled ? .neutral : (mode == "closed" ? .danger : (mode == "open" ? .success : .accent))
        let modeLabel = t("delivery.mode_\(mode)")
        return Button {
            guard enabled, !app.isBusy else { return }
            Task { await app.cycleAccepting(channel: channel, current: mode) }
        } label: {
            StatusChip(label: "\(label): \(modeLabel)", tone: tone)
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.5)
    }

    @ViewBuilder private var content: some View {
        if app.isLoadingDelivery && app.deliveryOrders.isEmpty {
            ScrollView { SkeletonList() }
        } else if app.deliveryOrders.isEmpty {
            VStack(spacing: Space.md) {
                Image(systemName: "bicycle").font(.system(size: 40, weight: .light)).foregroundStyle(theme.colors.textMuted)
                Text(t("delivery.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                VStack(spacing: Space.sm) {
                    ForEach(app.deliveryOrders, id: \.id) { order in
                        DeliveryOrderCard(
                            app: app, order: order,
                            onFinalize: { finalizing = order },
                            onCancel: { cancelling = order }
                        )
                    }
                }
                .frame(maxWidth: 620).frame(maxWidth: .infinity).padding(Space.lg)
            }
        }
    }
}

// MARK: - Order card

private struct DeliveryOrderCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let order: DeliveryOrderView
    let onFinalize: () -> Void
    let onCancel: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack(spacing: Space.sm) {
                StatusChip(label: t("delivery.status.\(order.status)"), tone: statusTone(order.status))
                StatusChip(label: t("delivery.\(order.channel)"), tone: .neutral)
                Spacer()
                if let ref = order.orderRef {
                    Text(ref).font(.money(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                }
            }
            HStack {
                Text(order.customerName).font(.ui(15, .bold)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                Text(Money.format(order.totalMinor, currency)).font(.money(15, .heavy)).foregroundStyle(theme.colors.accent)
            }
            Text(order.customerPhone).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
            if let addr = order.address {
                Text(addr).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
            }
            HStack(spacing: Space.sm) {
                Text("\(order.itemCount) \(t("delivery.items"))").font(.ui(11, .medium)).foregroundStyle(theme.colors.textMuted)
                if order.deliveryFeeMinor > 0 {
                    Text("· \(t("receipt.delivery_fee")) \(Money.format(order.deliveryFeeMinor, currency))")
                        .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                }
            }
            if !order.isTerminal { actions }
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous).strokeBorder(theme.colors.border, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }

    private var actions: some View {
        HStack(spacing: Space.sm) {
            if let next = nextStatus(order.status) {
                SufrixButton(label: t("delivery.action.\(next)"), icon: "arrow.right.circle", fullWidth: false) {
                    Task { await app.advanceDelivery(order) }
                }
            }
            Spacer()
            Menu {
                Button { Task { await app.addDeliveryPrep(order) } } label: { Label(t("delivery.add_prep"), systemImage: "clock") }
                Button { onFinalize() } label: { Label(t("delivery.finalize"), systemImage: "checkmark.seal") }
                Button(role: .destructive) { onCancel() } label: { Label(t("delivery.cancel"), systemImage: "xmark.circle") }
            } label: {
                Image(systemName: "ellipsis.circle").font(.system(size: 22)).foregroundStyle(theme.colors.textSecondary)
            }
        }
        .padding(.top, 2)
    }
}

// MARK: - Finalize (payment picker)

private struct FinalizeSheet: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let order: DeliveryOrderView
    let onClose: () -> Void
    @State private var method: String?

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: Space.lg) {
            Text(t("delivery.finalize")).font(.ui(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Text("\(order.customerName) · \(Money.format(order.totalMinor, currency))")
                .font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Text(t("delivery.finalize_pay")).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                .frame(maxWidth: .infinity, alignment: .leading)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: Space.sm)], spacing: Space.sm) {
                ForEach(app.paymentMethods, id: \.id) { m in
                    SufrixButton(label: m.name, variant: m.id == method ? .primary : .outline) { method = m.id }
                }
            }
            SufrixButton(label: t("delivery.finalize"), icon: "checkmark.seal", loading: app.isBusy) {
                guard let id = method else { return }
                Task { if await app.finalizeDelivery(order, paymentMethodId: id) { onClose() } }
            }
            .disabled(method == nil)
            Spacer()
        }
        .padding(Space.xl)
        .frame(maxWidth: 460)
        .onAppear { method = (app.paymentMethods.first { $0.isCash } ?? app.paymentMethods.first)?.id }
    }
}

// MARK: - Cancel (reason + restock)

private struct CancelSheet: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let order: DeliveryOrderView
    let onClose: () -> Void
    @State private var reason = ""
    @State private var restock = true

    var body: some View {
        VStack(spacing: Space.lg) {
            Text(t("delivery.cancel")).font(.ui(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Text(order.customerName).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            SufrixTextField(placeholder: t("delivery.cancel_reason"), text: $reason, icon: "text.bubble")
            Toggle(isOn: $restock) {
                Text(t("delivery.restore_inventory")).font(.ui(14)).foregroundStyle(theme.colors.textPrimary)
            }
            .tint(theme.colors.accent)
            SufrixButton(label: t("delivery.cancel"), icon: "xmark.circle", variant: .danger, loading: app.isBusy) {
                Task {
                    if await app.cancelDelivery(order, reason: reason.isEmpty ? nil : reason, restoreInventory: restock) { onClose() }
                }
            }
            Spacer()
        }
        .padding(Space.xl)
        .frame(maxWidth: 460)
    }
}

// MARK: - helpers

private func nextStatus(_ s: String) -> String? {
    switch s {
    case "received": return "confirmed"
    case "confirmed": return "preparing"
    case "preparing": return "ready"
    case "ready": return "out_for_delivery"
    case "out_for_delivery": return "delivered"
    default: return nil
    }
}

private func statusTone(_ s: String) -> ChipTone {
    switch s {
    case "received": return .info
    case "confirmed", "out_for_delivery": return .accent
    case "preparing": return .warning
    case "ready", "delivered": return .success
    case "cancelled", "rejected": return .danger
    default: return .neutral
    }
}

extension DeliveryOrderView: Identifiable {}
