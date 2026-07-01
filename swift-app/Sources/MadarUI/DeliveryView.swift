// Delivery queue — the teller works a branch's live delivery orders: advance the
// lifecycle (Confirm → Preparing → Ready → Out for delivery → Delivered), bump
// prep time, cancel (with restock), and finalize into a real sale on the open
// shift. All logic is in the core; this view only renders + collects. Online —
// it refreshes on appear and on a light poll while open.
import SwiftUI

// Delivery queue body — the "Delivery" tab of the unified Orders surface. No nav
// header of its own (IncomingView owns back + title + the tab bar); just the
// Active/All toolbar + accepting chips + the live list. Live via the shared
// `app.deliveryOrders` (onRealtimeEvent → loadDeliveryOrders refreshes it).
struct DeliveryBody: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var finalizing: DeliveryOrderView?
    @State private var cancelling: DeliveryOrderView?
    @State private var viewing: DeliveryOrderView?

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                toolbar
                if let s = app.deliverySettings { acceptingBar(s) }
                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning).padding(Space.lg)
                }
                content
            }
        }
        .task { await app.loadDeliveryOrders() }
        // SSE is primary now (delivery events arrive on the session-level subscription
        // → onRealtimeEvent → loadDeliveryOrders). This slow poll is just a safety net
        // for a missed event / dropped stream.
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 60_000_000_000)
                if Task.isCancelled { break }
                await app.loadDeliveryOrders()
            }
        }
        // Finalize — the full-height details → shared CheckoutDrawer flow, the SAME
        // drawer the cashier checkout and ticket settle use (mirrors SettleSheet).
        .madarSheet(item: $finalizing, size: .large, maxWidth: 560) { order, dismiss in
            FinalizeSheet(app: app, order: order, onClose: dismiss)
        }
        .madarSheet(item: $cancelling, maxWidth: 520) { order, dismiss in
            CancelSheet(app: app, order: order, onClose: dismiss)
        }
        // Order details — the SAME layout as the ticket details sheet (P1/P3).
        .madarSheet(item: $viewing, size: .large, maxWidth: 560) { order, dismiss in
            DeliveryDetailsSheet(order: order, currency: app.session?.currencyCode ?? "", onClose: dismiss)
        }
    }

    private var toolbar: some View {
        HStack {
            Spacer()
            // Active-only ↔ all toggle.
            Picker("", selection: $app.deliveryActiveOnly) {
                Text(t("delivery.active")).tag(true)
                Text(t("delivery.all")).tag(false)
            }
            .pickerStyle(.segmented)
            .frame(width: 170)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.sm)
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
        .buttonStyle(.pressable)
        .opacity(enabled ? 1 : 0.5)
    }

    @ViewBuilder private var content: some View {
        if app.isLoadingDelivery && app.deliveryOrders.isEmpty {
            ScrollView { SkeletonList() }
        } else if app.deliveryOrders.isEmpty {
            VStack(spacing: Space.md) {
                MadarIcon("bicycle", size: 40).foregroundStyle(theme.colors.textMuted)
                Text(t("delivery.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(spacing: Space.sm) {
                    ForEach(app.deliveryOrders, id: \.id) { order in
                        DeliveryOrderCard(
                            app: app, order: order,
                            onView: { viewing = order },
                            onFinalize: { finalizing = order },
                            onCancel: { cancelling = order },
                            onReject: { Task { _ = await app.rejectDelivery(order) } }
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
    let onView: () -> Void
    let onFinalize: () -> Void
    let onCancel: () -> Void
    let onReject: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            statusStrip
            VStack(alignment: .leading, spacing: Space.sm) {
                customerHeader
                if let addr = order.address {
                    Text(addr).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
                }
                // Customer delivery instructions ("leave at door", "call on arrival") —
                // fulfillment-critical text the core carries but neither host rendered.
                if let note = order.deliveryNotes, !note.isEmpty {
                    deliveryNote(note)
                }
                metaLine
                // "View order" — opens the shared details layout (P1). Always
                // available (even terminal orders, for lookup).
                MadarButton(label: t("order.view_order"), icon: "list.bullet.rectangle", variant: .outline) { onView() }
                if !order.isTerminal { actions }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(Space.md)
        }
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }

    // Status-tinted header strip — fixed height so every card's body starts at the
    // same y and the lifecycle status reads at a glance (mirrors the Kitchen ticket's
    // age-tinted strip). Status dot + bold label lead; channel chip + order ref follow.
    private var statusStrip: some View {
        let tint = statusTint(order.status, theme.colors)
        return HStack(spacing: Space.sm) {
            Circle().fill(tint.fg).frame(width: 8, height: 8)
            Text(t("delivery.status.\(order.status)")).font(.ui(14, .black)).foregroundStyle(tint.fg).lineLimit(1)
            StatusChip(label: t("delivery.\(order.channel)"), tone: .neutral)
            Spacer()
            if let ref = order.orderRef {
                Text(ref).font(.money(13, .bold)).foregroundStyle(tint.fg)
            }
        }
        .padding(.horizontal, Space.md)
        .frame(height: 50)
        .frame(maxWidth: .infinity)
        .background(tint.bg)
    }

    // Leading person tone-tile + name/phone, money as the hero in a tinted teal block.
    private var customerHeader: some View {
        HStack(spacing: Space.sm) {
            MadarIcon("person.fill", size: IconSize.lg)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 40, height: 40)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(order.customerName).font(.ui(16, .bold)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                Text(order.customerPhone).font(.ui(12, .medium)).foregroundStyle(theme.colors.textSecondary)
            }
            Spacer(minLength: Space.sm)
            Text(Money.format(order.totalMinor, currency))
                .font(.money(17, .heavy)).foregroundStyle(theme.colors.accent)
                .padding(.horizontal, Space.md).padding(.vertical, 7)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
    }

    private func deliveryNote(_ note: String) -> some View {
        HStack(alignment: .top, spacing: Space.xs) {
            MadarIcon("text.bubble", size: IconSize.sm).foregroundStyle(theme.colors.warning)
            Text(note).font(.ui(12, .medium)).foregroundStyle(theme.colors.warning)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Space.sm).padding(.vertical, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.colors.warningBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }

    private var metaLine: some View {
        HStack(spacing: Space.sm) {
            Text("\(order.itemCount) \(t("delivery.items"))").font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted)
            if order.deliveryFeeMinor > 0 {
                Text("· \(t("receipt.delivery_fee")) \(Money.format(order.deliveryFeeMinor, currency))")
                    .font(.money(11, .medium)).foregroundStyle(theme.colors.textMuted)
            }
        }
    }

    private func advance() { Task { await app.advanceDelivery(order) } }
    private func addPrep() { Task { await app.addDeliveryPrep(order) } }

    private var actions: some View {
        HStack(spacing: Space.sm) {
            if let next = nextStatus(order.status) {
                MadarButton(label: t("delivery.action.\(next)"), icon: "arrow.right.circle", fullWidth: false, action: advance)
            }
            Spacer()
            Menu {
                Button(action: addPrep) { Label(t("delivery.add_prep"), systemImage: "clock") }
                Button(action: onFinalize) { Label(t("delivery.finalize"), systemImage: "checkmark.seal") }
                if order.status == "received" {
                    Button(role: .destructive, action: onReject) { Label(t("delivery.reject"), systemImage: "hand.raised") }
                }
                Button(role: .destructive, action: onCancel) { Label(t("delivery.cancel"), systemImage: "xmark.circle") }
            } label: {
                MadarIcon("ellipsis", size: IconSize.lg)
                    .foregroundStyle(theme.colors.textSecondary)
                    .frame(width: 34, height: 34)
                    .background(theme.colors.surfaceAlt)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(theme.colors.borderLight, lineWidth: 1))
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
        }
        .padding(.top, 2)
    }
}

// MARK: - Finalize (details → shared checkout drawer)

/// The two-step finalize sheet — the delivery counterpart to `SettleSheet`, so the
/// delivery finalize, the cashier checkout, and the ticket settle all route through
/// the ONE shared `CheckoutDrawer` (no more mirrored delivery payment picker).
/// STEP 1 shows the real order details (frozen lines + money + fulfillment context);
/// STEP 2 hands off to the SHARED drawer, whose terminal finalizes the delivery into
/// a paid order on the open shift via `app.finalizeDelivery`.
private struct FinalizeSheet: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let order: DeliveryOrderView
    let onClose: () -> Void

    private enum Step { case details, checkout }
    @State private var step: Step = .details

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        switch step {
        case .details:
            detailsStep
        case .checkout:
            // The SAME drawer the main cashier + ticket settle use. `.flat` summary
            // on the delivery total (order.totalMinor INCLUDES the delivery fee, so
            // cash-due + change math is correct). The backend finalize only needs the
            // payment method (shift resolved inside finalizeDelivery), so the drawer's
            // tip/split/cash-tendered extras are cashier aid only — ignored here. No
            // cart-discount edit, no customer capture (the order already knows both).
            CheckoutDrawer(
                app: app,
                title: order.orderRef ?? order.customerName,
                total: order.totalMinor,
                currency: currency,
                busy: app.isBusy,
                terminalLabel: t("delivery.finalize"),
                terminalIcon: "checkmark.seal",
                errorMessage: app.errorMessage,
                summary: .flat,
                showCartDiscount: false,
                showCustomerCapture: false,
                onClose: onClose,
                onTerminal: { input in
                    if await app.finalizeDelivery(order, paymentMethodId: input.paymentMethodId) {
                        onClose()
                    }
                })
        }
    }

    private var detailsStep: some View {
        VStack(spacing: 0) {
            // Surface the real priced lines + fulfillment context via the shared
            // DeliveryDetailsView (same layout as the "View order" sheet).
            ScrollView {
                DeliveryDetailsView(order: order, currency: currency)
                    .frame(maxWidth: 552).frame(maxWidth: .infinity)
                    .padding(.horizontal, Space.xl)
                    .padding(.top, Space.md)
                    .padding(.bottom, Space.lg)
            }
            // Advance to the shared checkout drawer.
            VStack(spacing: Space.sm) {
                MadarButton(label: t("delivery.finalize"), icon: "checkmark.seal") {
                    withAnimation(Motion.standard) { step = .checkout }
                }
            }
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
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

    private func cancel() {
        Task {
            if await app.cancelDelivery(order, reason: reason.isEmpty ? nil : reason, restoreInventory: restock) { onClose() }
        }
    }

    var body: some View {
        VStack(spacing: Space.lg) {
            Text(t("delivery.cancel")).typo(.h2).foregroundStyle(theme.colors.textPrimary)
            Text(order.customerName).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            MadarTextField(placeholder: t("delivery.cancel_reason"), text: $reason, icon: "text.bubble")
            Toggle(isOn: $restock) {
                Text(t("delivery.restore_inventory")).font(.ui(14)).foregroundStyle(theme.colors.textPrimary)
            }
            .tint(theme.colors.accent)
            MadarButton(label: t("delivery.cancel"), icon: "xmark.circle", variant: .danger, loading: app.isBusy, action: cancel)
            Spacer()
        }
        .frame(maxWidth: 460)
        .frame(maxWidth: .infinity)
        .padding(Space.xl)
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

// Status → (foreground, tinted-background) for the card's header strip. Mirrors the
// Kitchen ticket's age-tint pattern so the lifecycle reads at a glance.
private func statusTint(_ s: String, _ c: MadarColors) -> (fg: Color, bg: Color) {
    switch s {
    case "received": return (c.navy, c.navyBg)
    case "confirmed", "out_for_delivery": return (c.accent, c.accentBg)
    case "preparing": return (c.warning, c.warningBg)
    case "ready", "delivered": return (c.success, c.successBg)
    case "cancelled", "rejected": return (c.danger, c.dangerBg)
    default: return (c.textSecondary, c.surfaceAlt)
    }
}

// MARK: - Details sheet

/// Scrollable wrapper that presents the shared `DeliveryDetailsView` in a sheet —
/// the delivery counterpart to the ticket details step, so both Orders tabs route
/// through the same details layout (P1/P3).
private struct DeliveryDetailsSheet: View {
    let order: DeliveryOrderView
    let currency: String
    let onClose: () -> Void

    var body: some View {
        ScrollView {
            DeliveryDetailsView(order: order, currency: currency)
                .frame(maxWidth: 552).frame(maxWidth: .infinity)
                .padding(.horizontal, Space.xl)
                .padding(.top, Space.md)
                .padding(.bottom, Space.lg)
        }
    }
}

extension DeliveryOrderView: Identifiable {}
