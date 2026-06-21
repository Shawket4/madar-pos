// Tender — the checkout sheet. Pick a payment method (or split a bill), take cash
// with live change, add a tip/discount, then place the order through the core
// (online or queued offline). On success the same sheet flips to a receipt
// confirmation with a printable on-screen preview. All money + order assembly
// live in the core; this view only collects the tender and renders.
import SwiftUI

struct TenderView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var selectedMethod: String?
    @State private var tenderedMinor: Int64 = 0
    @State private var tipMinor: Int64 = 0
    @State private var tipMethod: String?
    @State private var customerName = ""
    @State private var notes = ""
    /// Split a single bill across several payment methods (the teller allocates
    /// an amount per method that must sum to the total).
    @State private var splitMode = false
    @State private var splitAmounts: [String: Int64] = [:] // methodId → allocated

    private var currency: String { app.session?.currencyCode ?? "" }
    private var method: PaymentMethodView? { app.paymentMethods.first { $0.id == selectedMethod } }
    private var isCash: Bool { method?.isCash ?? false }
    private var total: Int64 { app.cartTotals.totalMinor }
    /// A tip paid on a cash order comes out of the same drawer → due with the bill.
    private var tipCash: Int64 { (tipMethodIsCash) ? tipMinor : 0 }
    private var tipMethodIsCash: Bool {
        guard tipMinor > 0 else { return false }
        let m = app.paymentMethods.first { $0.id == (tipMethod ?? selectedMethod) }
        return m?.isCash ?? isCash
    }
    private var dueCash: Int64 { total + tipCash }
    private var changeMinor: Int64 { max(0, tenderedMinor - dueCash) }
    private var shortMinor: Int64 { max(0, dueCash - tenderedMinor) }

    // ── split payment ──
    private var splitAllocated: Int64 { splitAmounts.values.reduce(0, +) }
    private var splitRemaining: Int64 { total - splitAllocated }
    private var splitLegs: [CheckoutSplit] {
        splitAmounts.compactMap { id, amt in amt > 0 ? CheckoutSplit(paymentMethodId: id, amountMinor: amt) : nil }
    }
    /// The biggest leg is recorded as the order's primary method.
    private var splitPrimary: String? {
        splitAmounts.filter { $0.value > 0 }.max { $0.value < $1.value }?.key
    }
    private func splitBinding(_ id: String) -> Binding<Int64> {
        Binding(get: { splitAmounts[id] ?? 0 }, set: { splitAmounts[id] = $0 })
    }

    private var canPlace: Bool {
        if app.isPlacingOrder { return false }
        if splitMode { return splitAllocated == total && !splitLegs.isEmpty }
        return selectedMethod != nil && (!isCash || tenderedMinor >= dueCash)
    }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            if let receipt = app.receipt {
                ReceiptConfirmation(app: app, receipt: receipt, currency: currency) { onClose() }
            } else {
                tenderForm
            }
        }
        .onAppear {
            if selectedMethod == nil {
                selectedMethod = (app.paymentMethods.first { $0.isCash } ?? app.paymentMethods.first)?.id
            }
        }
    }

    private func discountLabel(_ d: DiscountView) -> String {
        d.dtype == "percentage" ? "\(d.name) \(d.value)%" : d.name
    }

    private func place() async {
        let name = customerName.isEmpty ? nil : customerName
        let note = notes.isEmpty ? nil : notes
        if splitMode {
            guard let primary = splitPrimary else { return }
            await app.placeOrder(paymentMethodId: primary, amountTenderedMinor: 0, tipMinor: tipMinor,
                                 tipPaymentMethodId: tipMethod, customerName: name, notes: note, splits: splitLegs)
        } else {
            guard let id = selectedMethod else { return }
            await app.placeOrder(paymentMethodId: id, amountTenderedMinor: isCash ? tenderedMinor : 0,
                                 tipMinor: tipMinor, tipPaymentMethodId: tipMethod, customerName: name, notes: note)
        }
    }

    // MARK: - Form

    private var tenderForm: some View {
        VStack(spacing: 0) {
            // Header.
            HStack {
                Text(t("order.tender")).font(.ui(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                Button { onClose() } label: {
                    Image(systemName: "xmark").font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(theme.colors.textMuted)
                        .frame(width: 32, height: 32)
                        .background(theme.colors.surfaceAlt).clipShape(Circle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, Space.lg).padding(.top, Space.sm).padding(.bottom, Space.md)

            ScrollView {
                VStack(spacing: Space.lg) {
                    summaryCard
                    paymentSection
                    if isCash && !splitMode { cashSection }
                    tipCard
                    discountSection
                    customerSection
                    if let error = app.errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    }
                }
                .frame(maxWidth: 480)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Space.lg)
                .padding(.bottom, Space.lg)
            }

            footer
        }
    }

    /// Order totals card (always visible at the top of the sheet).
    private var summaryCard: some View {
        VStack(spacing: Space.sm) {
            row(t("order.subtotal"), Money.format(app.cartTotals.subtotalMinor, currency), tone: theme.colors.textSecondary)
            if app.cartTotals.discountMinor > 0 {
                row(t("order.discount"), "−\(Money.format(app.cartTotals.discountMinor, currency))", tone: theme.colors.success)
            }
            if app.cartTotals.taxMinor > 0 {
                row(t("order.tax"), Money.format(app.cartTotals.taxMinor, currency), tone: theme.colors.textSecondary)
            }
            Rectangle().fill(theme.colors.border).frame(height: 1)
            row(t("order.total"), Money.format(total, currency), emphasized: true)
        }
        .padding(Space.lg)
        .background(theme.colors.surfaceAlt)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }

    private var paymentSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack {
                sectionLabel(t("order.payment_method"))
                Spacer()
                if app.paymentMethods.count > 1 {
                    Button { Haptics.selection(); withAnimation(Motion.standard) { splitMode.toggle() } } label: {
                        HStack(spacing: 4) {
                            Image(systemName: splitMode ? "checkmark.circle.fill" : "rectangle.split.2x1")
                            Text(t("order.split_payment"))
                        }
                        .font(.ui(11, .semibold))
                        .foregroundStyle(splitMode ? theme.colors.accent : theme.colors.textMuted)
                        .padding(.horizontal, 8).padding(.vertical, 4)
                        .background(splitMode ? theme.colors.accentBg : theme.colors.surfaceAlt)
                        .clipShape(Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            if splitMode {
                splitAllocator
            } else {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: Space.sm)], spacing: Space.sm) {
                    ForEach(app.paymentMethods, id: \.id) { m in
                        PayChip(method: m, active: m.id == selectedMethod) { selectedMethod = m.id }
                    }
                }
            }
        }
    }

    /// Per-method amount entry + a live remaining indicator (must reach 0).
    private var splitAllocator: some View {
        VStack(spacing: Space.sm) {
            ForEach(app.paymentMethods, id: \.id) { m in
                HStack(spacing: Space.sm) {
                    Circle().fill(Color(hex: m.color)).frame(width: 9, height: 9)
                    Text(m.name).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                        .frame(width: 86, alignment: .leading)
                    AmountField(amountMinor: splitBinding(m.id), currencyCode: currency)
                }
            }
            HStack {
                Text(t("order.split_remaining")).font(.ui(12, .medium)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
                Text(Money.format(splitRemaining, currency))
                    .font(.money(13, .bold))
                    .foregroundStyle(splitRemaining == 0 ? theme.colors.success : theme.colors.danger)
            }
            .padding(.horizontal, Space.md).padding(.vertical, 10)
            .background((splitRemaining == 0 ? theme.colors.successBg : theme.colors.warningBg))
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
    }

    private var cashSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionLabel(t("order.cash_received"))
            AmountField(amountMinor: $tenderedMinor, currencyCode: currency)
            // Quick-tender chips.
            FlowLayout(spacing: Space.sm) {
                quickChip(t("order.exact"), amount: dueCash)
                ForEach(quickPresets, id: \.self) { p in quickChip(Money.format(p, currency), amount: p) }
            }
            if tenderedMinor > 0 {
                changeBanner
            }
        }
    }

    /// Round-number cash presets at or above the amount due.
    private var quickPresets: [Int64] {
        let units: [Int64] = [5000, 10000, 20000, 50000] // 50/100/200/500 major
        return units.filter { $0 >= dueCash }.prefix(3).map { $0 }
    }

    private func quickChip(_ label: String, amount: Int64) -> some View {
        let active = tenderedMinor == amount
        return Button { Haptics.selection(); tenderedMinor = amount } label: {
            Text(label)
                .font(.ui(12, .bold))
                .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textSecondary)
                .padding(.horizontal, 14).padding(.vertical, 7)
                .background(active ? theme.colors.accent : theme.colors.surfaceAlt)
                .clipShape(Capsule())
                .overlay(Capsule().strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.96))
    }

    @ViewBuilder private var changeBanner: some View {
        let ok = changeMinor >= 0 && shortMinor == 0
        HStack(spacing: Space.sm) {
            Image(systemName: ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .foregroundStyle(ok ? theme.colors.success : theme.colors.danger)
            Text(ok ? t("order.change_due") : t("order.short_by"))
                .font(.ui(13, .semibold)).foregroundStyle(ok ? theme.colors.success : theme.colors.danger)
            Spacer()
            Text(Money.format(ok ? changeMinor : shortMinor, currency))
                .font(.money(15, .heavy)).foregroundStyle(ok ? theme.colors.success : theme.colors.danger)
        }
        .padding(.horizontal, Space.md).padding(.vertical, 10)
        .background(ok ? theme.colors.successBg : theme.colors.dangerBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }

    private var tipCard: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack(spacing: 6) {
                Image(systemName: "heart.circle").font(.system(size: 13))
                sectionLabel(t("order.tip"))
                Spacer()
                if tipMinor > 0 {
                    StatusChip(label: Money.format(tipMinor, currency), icon: "plus", tone: .success)
                }
            }
            if app.paymentMethods.count > 1 {
                FlowLayout(spacing: 6) {
                    ForEach(app.paymentMethods, id: \.id) { m in
                        let active = (tipMethod ?? selectedMethod) == m.id
                        Button { Haptics.selection(); tipMethod = m.id } label: {
                            HStack(spacing: 4) {
                                if active { Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)) }
                                Text(m.name).font(.ui(11, .semibold))
                            }
                            .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textSecondary)
                            .padding(.horizontal, 11).padding(.vertical, 6)
                            .background(active ? Color(hex: m.color) : theme.colors.surface)
                            .clipShape(Capsule())
                            .overlay(Capsule().strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1))
                        }
                        .buttonStyle(.pressable(scale: 0.96))
                    }
                }
            }
            AmountField(amountMinor: $tipMinor, currencyCode: currency)
        }
        .padding(Space.lg)
        .background(theme.colors.surfaceAlt)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.border, lineWidth: 1))
    }

    @ViewBuilder private var discountSection: some View {
        let activeDiscounts = app.discounts.filter { $0.isActive }
        if !activeDiscounts.isEmpty {
            VStack(alignment: .leading, spacing: Space.sm) {
                sectionLabel(t("order.discount"))
                FlowLayout(spacing: Space.sm) {
                    chip(t("order.no_discount"), active: app.cartDiscountId == nil) { app.setDiscount(nil) }
                    ForEach(activeDiscounts, id: \.id) { d in
                        chip(discountLabel(d), active: app.cartDiscountId == d.id) { app.setDiscount(d.id) }
                    }
                }
            }
        }
    }

    private var customerSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionLabel(t("order.customer"))
            SufrixTextField(placeholder: t("order.customer_hint"), text: $customerName, icon: "person", caps: .words)
            SufrixTextField(placeholder: t("order.notes_hint"), text: $notes, icon: "text.bubble", caps: .words)
        }
    }

    private var footer: some View {
        VStack(spacing: Space.sm) {
            SufrixButton(label: t("order.place_order"), icon: "checkmark", loading: app.isPlacingOrder) {
                Task { await place() }
            }
            .opacity(canPlace ? 1 : 0.5)
            .allowsHitTesting(canPlace)
            .keyboardShortcut(.return, modifiers: .command)
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    // MARK: small builders
    private func sectionLabel(_ s: String) -> some View {
        Text(s).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
    }

    private func row(_ label: String, _ value: String, emphasized: Bool = false, tone: Color? = nil) -> some View {
        HStack {
            Text(label)
                .font(.ui(emphasized ? 16 : 14, emphasized ? .bold : .medium))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : (tone ?? theme.colors.textSecondary))
            Spacer()
            Text(value)
                .font(.money(emphasized ? 18 : 14, emphasized ? .heavy : .semibold))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : (tone ?? theme.colors.textSecondary))
                .contentTransition(.numericText())
        }
    }

    private func chip(_ label: String, active: Bool, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: 5) {
                if active { Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)) }
                Text(label).font(.ui(13, .semibold))
            }
            .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .background(active ? theme.colors.accent : theme.colors.surfaceAlt)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: active ? 0 : 1))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }
}

/// Payment method tile — icon (in the method's brand color) + label + a check
/// when active. Selected fills with the method color.
private struct PayChip: View {
    @Environment(\.theme) private var theme
    let method: PaymentMethodView
    let active: Bool
    let action: () -> Void

    private var color: Color { Color(hex: method.color) }

    var body: some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: Space.sm) {
                Image(systemName: PayChip.symbol(method.icon))
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(active ? theme.colors.textOnAccent : color)
                Text(method.name).font(.ui(13, .semibold)).lineLimit(1)
                    .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
                Spacer(minLength: 0)
                if active { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold))
                    .foregroundStyle(theme.colors.textOnAccent) }
            }
            .padding(.horizontal, 12).padding(.vertical, 13)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(active ? color : theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }

    /// Map a backend payment-icon token to an SF Symbol.
    static func symbol(_ icon: String) -> String {
        switch icon.lowercased() {
        case "cash", "banknote", "money": return "banknote"
        case "card", "credit_card", "creditcard", "visa", "mastercard", "debit": return "creditcard"
        case "wallet", "ewallet", "e_wallet": return "wallet.pass"
        case "bank", "transfer", "bank_transfer": return "building.columns"
        case "phone", "mobile", "vodafone", "instapay": return "iphone"
        case "qr", "qr_code": return "qrcode"
        default: return "dollarsign.circle"
        }
    }
}

// MARK: - Receipt confirmation (preview before printing)

private struct ReceiptConfirmation: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let receipt: ReceiptView
    let currency: String
    let onDone: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            // Status header.
            VStack(spacing: Space.sm) {
                Image(systemName: receipt.queuedOffline ? "clock.badge.checkmark" : "checkmark.circle.fill")
                    .font(.system(size: 38))
                    .foregroundStyle(receipt.queuedOffline ? theme.colors.warning : theme.colors.success)
                Text(t("order.order_placed")).font(.ui(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
                StatusChip(
                    label: t(receipt.queuedOffline ? "order.queued_hint" : "order.sent_hint"),
                    icon: receipt.queuedOffline ? "clock" : "checkmark.circle",
                    tone: receipt.queuedOffline ? .warning : .success
                )
            }
            .padding(.top, Space.lg).padding(.bottom, Space.md)

            // The printable receipt, exactly as it will print.
            ScrollView {
                ReceiptPaper(receipt: receipt, storeName: app.branchName, currency: currency)
                    .padding(.horizontal, Space.lg)
                    .padding(.bottom, Space.lg)
            }

            // Actions.
            VStack(spacing: Space.sm) {
                printControl
                SufrixButton(label: t("order.new_order"), icon: "plus", variant: .outline) { onDone() }
            }
            .padding(Space.lg)
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
    }

    /// Print receipt — best-effort send to the configured network printer, with
    /// inline state (printing / sent / unreachable / not-configured).
    @ViewBuilder private var printControl: some View {
        switch app.printState {
        case .printed:
            StatusChip(label: t("receipt.printed"), icon: "checkmark.circle", tone: .success)
                .frame(maxWidth: .infinity)
        case .noPrinter:
            StatusChip(label: t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning)
                .frame(maxWidth: .infinity)
        default:
            SufrixButton(
                label: app.printState == .failed ? t("receipt.print_failed") : t("receipt.print"),
                icon: "printer",
                loading: app.printState == .printing
            ) {
                Task { await app.printCurrentReceipt() }
            }
        }
    }
}
