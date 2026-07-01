// Tender / checkout — the ONE shared payment drawer.
//
// `CheckoutDrawer` is the reusable core: it collects the tender (payment method
// or split, cash with live change, tip) against a fixed `total` and reports the
// assembled `CheckoutTerminalInput` to its owner via `onTerminal`. The MAIN
// cashier checkout (`TenderView`, presented from `OrderView`) and the ticket
// SETTLE flow (`SettleSheet`, presented from the Orders surface) both
// drive the SAME `CheckoutDrawer` — no more mirrored settle UI. Each owner wires
// the terminal action to the right core call (place order vs. settle ticket) and
// supplies the fixed order-summary block above the form.
//
// All money + order assembly live in the core; this view only collects + renders.
import SwiftUI

/// The tender the teller collected in the drawer, handed back to the owner's
/// terminal action. `splits` is non-empty only in split-payment mode; otherwise
/// `paymentMethodId` + `amountTenderedMinor` describe a single-method payment.
struct CheckoutTerminalInput {
    let paymentMethodId: String
    let amountTenderedMinor: Int64
    let tipMinor: Int64
    let tipPaymentMethodId: String?
    let customerName: String?
    let notes: String?
    let splits: [CheckoutSplit]
    /// Whether the selected primary method is cash (owners that don't take a
    /// tendered amount for card can branch on this).
    let isCash: Bool
}

// MARK: - TenderView (MAIN cashier checkout — presented from OrderView)

struct TenderView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }
    private var total: Int64 { app.cartTotals.totalMinor }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            if let receipt = app.receipt {
                ReceiptConfirmation(app: app, receipt: receipt, currency: currency) { onClose() }
            } else {
                // The MAIN checkout: full breakdown summary + editable cart
                // discount + a customer/notes capture; the terminal places the cart.
                CheckoutDrawer(
                    app: app,
                    title: t("order.tender"),
                    total: total,
                    currency: currency,
                    busy: app.isPlacingOrder,
                    terminalLabel: t("order.place_order"),
                    terminalIcon: "checkmark",
                    errorMessage: app.errorMessage,
                    summary: .totals(app.cartTotals),
                    showCartDiscount: true,
                    showCustomerCapture: true,
                    onClose: onClose,
                    onTerminal: { input in
                        await app.placeOrder(
                            paymentMethodId: input.paymentMethodId,
                            amountTenderedMinor: input.amountTenderedMinor,
                            tipMinor: input.tipMinor,
                            tipPaymentMethodId: input.tipPaymentMethodId,
                            customerName: input.customerName,
                            notes: input.notes,
                            splits: input.splits)
                    })
            }
        }
    }
}

// MARK: - CheckoutDrawer (the shared tender form)

/// How the fixed order-summary block above the form is rendered. The MAIN
/// checkout passes `.totals` for the full subtotal/discount/tax breakdown; the
/// settle flow passes `.flat` (just the grand total — the ticket carries only a
/// subtotal).
enum CheckoutSummary {
    case totals(CartTotals)
    case flat
}

/// The reusable payment drawer. Driven purely by a `total` + a terminal callback,
/// so any flow (cart checkout, ticket settle, …) reuses the exact same tender UI.
struct CheckoutDrawer: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    let title: String
    let total: Int64
    let currency: String
    let busy: Bool
    let terminalLabel: String
    let terminalIcon: String
    var errorMessage: String? = nil
    let summary: CheckoutSummary
    /// The MAIN checkout edits the cart's discount inline (via `app.setDiscount`).
    /// The settle flow does NOT (a ticket is settled at its frozen total).
    var showCartDiscount: Bool = false
    /// The MAIN checkout captures a walk-in customer name + order notes; settle
    /// already knows its ticket's covering customer, so it hides this.
    var showCustomerCapture: Bool = false
    let onClose: () -> Void
    let onTerminal: (CheckoutTerminalInput) async -> Void

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

    private var method: PaymentMethodView? { app.paymentMethods.first { $0.id == selectedMethod } }
    private var isCash: Bool { method?.isCash ?? false }
    /// A tip paid on a cash order comes out of the same drawer → due with the bill.
    private var tipCash: Int64 { tipMethodIsCash ? tipMinor : 0 }
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

    private var canPlace: Bool {
        if busy { return false }
        if splitMode { return splitAllocated == total && !splitLegs.isEmpty }
        return selectedMethod != nil && (!isCash || tenderedMinor >= dueCash)
    }

    var body: some View {
        VStack(spacing: 0) {
            TenderHeader(title: title, total: total, currency: currency, onClose: onClose)
            Rectangle().fill(theme.colors.border).frame(height: 1)

            ScrollView {
                VStack(spacing: Space.lg) {
                    summaryCard
                    PaymentSection(app: app, currency: currency, splitMode: $splitMode,
                                   selectedMethod: $selectedMethod, splitAmounts: $splitAmounts,
                                   splitRemaining: splitRemaining)
                    if isCash && !splitMode {
                        CashSection(dueCash: dueCash, tenderedMinor: $tenderedMinor,
                                    changeMinor: changeMinor, shortMinor: shortMinor, currency: currency)
                    }
                    TipCard(app: app, tipMinor: $tipMinor, tipMethod: $tipMethod,
                            selectedMethod: selectedMethod, currency: currency)
                    if showCartDiscount { DiscountSection(app: app) }
                    if showCustomerCapture { CustomerSection(customerName: $customerName, notes: $notes) }
                    if let errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: errorMessage, tone: .danger)
                    }
                }
                .frame(maxWidth: 552)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Space.xl)
                .padding(.top, Space.lg)
                .padding(.bottom, Space.lg)
            }

            footer
        }
        .onAppear {
            if selectedMethod == nil {
                selectedMethod = (app.paymentMethods.first { $0.isCash } ?? app.paymentMethods.first)?.id
            }
        }
    }

    @ViewBuilder private var summaryCard: some View {
        switch summary {
        case .totals(let totals): SummaryCard(totals: totals, total: total, currency: currency)
        case .flat: FlatTotalCard(total: total, currency: currency)
        }
    }

    private func fire() async {
        let name = customerName.isEmpty ? nil : customerName
        let note = notes.isEmpty ? nil : notes
        if splitMode {
            guard let primary = splitPrimary else { return }
            await onTerminal(CheckoutTerminalInput(
                paymentMethodId: primary, amountTenderedMinor: 0, tipMinor: tipMinor,
                tipPaymentMethodId: tipMethod, customerName: name, notes: note,
                splits: splitLegs, isCash: app.paymentMethods.first { $0.id == primary }?.isCash ?? false))
        } else {
            guard let id = selectedMethod else { return }
            await onTerminal(CheckoutTerminalInput(
                paymentMethodId: id, amountTenderedMinor: isCash ? tenderedMinor : 0, tipMinor: tipMinor,
                tipPaymentMethodId: tipMethod, customerName: name, notes: note, splits: [], isCash: isCash))
        }
    }

    private var footer: some View {
        VStack(spacing: Space.sm) {
            MadarButton(label: terminalLabel, icon: terminalIcon, loading: busy) {
                Task { await fire() }
            }
            .opacity(canPlace ? 1 : 0.5)
            .allowsHitTesting(canPlace)
            .keyboardShortcut(.return, modifiers: .command)
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

// MARK: - Header

/// Sticky sheet header — bold title + the live order total in hero teal + close.
private struct TenderHeader: View {
    @Environment(\.theme) private var theme
    let title: String
    let total: Int64
    let currency: String
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: Space.sm) {
            Text(title).font(.ui(19, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer()
            Text(Money.format(total, currency))
                .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
                .contentTransition(.numericText())
            Button { onClose() } label: {
                MadarIcon("xmark", size: 15)
                    .foregroundStyle(theme.colors.textMuted)
                    .frame(width: 32, height: 32)
                    .background(theme.colors.surfaceAlt).clipShape(Circle())
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Space.xl).padding(.top, Space.sm).padding(.bottom, Space.md)
    }
}

// MARK: - Summary card

/// Order totals card — subtotal/discount/tax in light muted rows, then the grand
/// total in a tinted teal block (bold teal figure). Matches the Order screen's
/// total block: the sub-rows stay light so the total carries the weight.
private struct SummaryCard: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let totals: CartTotals
    let total: Int64
    let currency: String

    var body: some View {
        VStack(spacing: Space.xs) {
            SummaryRow(label: t("order.subtotal"), value: Money.format(totals.subtotalMinor, currency))
            if totals.discountMinor > 0 {
                SummaryRow(label: t("order.discount"), value: "−\(Money.format(totals.discountMinor, currency))",
                           tone: theme.colors.success)
            }
            if totals.taxMinor > 0 {
                SummaryRow(label: t("order.tax"), value: Money.format(totals.taxMinor, currency))
            }
            // Grand-total block — tinted teal, the hero figure (matches CartFooter).
            HStack {
                Text(t("order.total")).font(.ui(14, .bold)).foregroundStyle(theme.colors.accent)
                Spacer()
                Text(Money.format(total, currency))
                    .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
                    .contentTransition(.numericText())
            }
            .padding(.horizontal, Space.md).padding(.vertical, Space.md)
            .background(theme.colors.accentBg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .padding(.top, Space.xs)
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

private struct SummaryRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var tone: Color? = nil

    var body: some View {
        HStack {
            Text(label).font(.ui(13, .medium)).foregroundStyle(tone ?? theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(tone ?? theme.colors.textSecondary)
                .contentTransition(.numericText())
        }
    }
}

/// A minimal amount-due card — just the grand-total block (no subtotal/tax rows).
/// Used by flows that only carry a single figure (ticket settle) so the drawer's
/// summary reads identically to the main checkout's hero total block.
private struct FlatTotalCard: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let total: Int64
    let currency: String

    var body: some View {
        HStack {
            Text(t("order.total")).font(.ui(14, .bold)).foregroundStyle(theme.colors.accent)
            Spacer()
            Text(Money.format(total, currency))
                .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
                .contentTransition(.numericText())
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.md)
        .background(theme.colors.accentBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

// MARK: - Payment

private struct PaymentSection: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let currency: String
    @Binding var splitMode: Bool
    @Binding var selectedMethod: String?
    @Binding var splitAmounts: [String: Int64]
    let splitRemaining: Int64

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack {
                sectionLabel(t("order.payment_method"))
                Spacer()
                if app.paymentMethods.count > 1 {
                    Button { Haptics.selection(); withAnimation(Motion.standard) { splitMode.toggle() } } label: {
                        HStack(spacing: 4) {
                            MadarIcon(splitMode ? "checkmark.circle.fill" : "rectangle.split.2x1", size: IconSize.md)
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
                SplitAllocator(app: app, currency: currency, splitAmounts: $splitAmounts, splitRemaining: splitRemaining)
            } else {
                MethodGrid(app: app, selectedMethod: $selectedMethod)
            }
        }
    }
}

/// Two-column grid of payment-method chips — the SHARED method selector used by
/// both the checkout and the settle sheet (adaptive LazyVGrid).
struct MethodGrid: View {
    @ObservedObject var app: AppModel
    @Binding var selectedMethod: String?

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: Space.sm)], spacing: Space.sm) {
            ForEach(app.paymentMethods, id: \.id) { m in
                PayChip(method: m, active: m.id == selectedMethod) { selectedMethod = m.id }
            }
        }
    }
}

/// Per-method amount entry + a live remaining indicator (must reach 0).
private struct SplitAllocator: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let currency: String
    @Binding var splitAmounts: [String: Int64]
    let splitRemaining: Int64

    private func binding(_ id: String) -> Binding<Int64> {
        Binding(get: { splitAmounts[id] ?? 0 }, set: { splitAmounts[id] = $0 })
    }

    var body: some View {
        VStack(spacing: Space.sm) {
            ForEach(app.paymentMethods, id: \.id) { m in
                HStack(spacing: Space.sm) {
                    Circle().fill(Color(hex: m.color)).frame(width: 9, height: 9)
                    Text(m.name).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                        .frame(width: 86, alignment: .leading)
                    AmountField(amountMinor: binding(m.id), currencyCode: currency)
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
            .background(splitRemaining == 0 ? theme.colors.successBg : theme.colors.warningBg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
    }
}

// MARK: - Cash

/// Cash tendered — a tinted teal "amount due" hero block, the cash field, round
/// presets, and a live change banner.
struct CashSection: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let dueCash: Int64
    @Binding var tenderedMinor: Int64
    let changeMinor: Int64
    let shortMinor: Int64
    let currency: String

    /// Round-number cash presets at or above the amount due.
    private var quickPresets: [Int64] {
        let units: [Int64] = [5000, 10000, 20000, 50000] // 50/100/200/500 major
        return units.filter { $0 >= dueCash }.prefix(3).map { $0 }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionLabel(t("order.cash_received"))
            // Amount-due hero block — tinted teal, the figure the cash must reach
            // (mirrors the grand-total block in weight + treatment).
            HStack {
                Text(t("order.total")).font(.ui(13, .bold)).foregroundStyle(theme.colors.accent)
                Spacer()
                Text(Money.format(dueCash, currency))
                    .font(.money(18, .heavy)).foregroundStyle(theme.colors.accent)
                    .contentTransition(.numericText())
            }
            .padding(.horizontal, Space.md).padding(.vertical, Space.md)
            .background(theme.colors.accentBg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            AmountField(amountMinor: $tenderedMinor, currencyCode: currency)
            FlowLayout(spacing: Space.sm) {
                QuickCash(label: t("order.exact"), amount: dueCash, tenderedMinor: $tenderedMinor)
                ForEach(quickPresets, id: \.self) { p in
                    QuickCash(label: Money.format(p, currency), amount: p, tenderedMinor: $tenderedMinor)
                }
            }
            if tenderedMinor > 0 {
                ChangeBanner(changeMinor: changeMinor, shortMinor: shortMinor, currency: currency)
            }
        }
    }
}

/// A quick-tender amount chip (Exact / round-number presets) that fills cash.
private struct QuickCash: View {
    @Environment(\.theme) private var theme
    let label: String
    let amount: Int64
    @Binding var tenderedMinor: Int64

    private var active: Bool { tenderedMinor == amount }

    var body: some View {
        Button { Haptics.selection(); tenderedMinor = amount } label: {
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
}

/// Green "Change due" / red "Short by" banner under the cash field — a leading
/// tone icon + the hero change figure.
private struct ChangeBanner: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let changeMinor: Int64
    let shortMinor: Int64
    let currency: String

    private var ok: Bool { changeMinor >= 0 && shortMinor == 0 }

    var body: some View {
        HStack(spacing: Space.sm) {
            MadarIcon(ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill", size: IconSize.lg)
                .foregroundStyle(ok ? theme.colors.success : theme.colors.danger)
            Text(ok ? t("order.change_due") : t("order.short_by"))
                .font(.ui(13, .semibold)).foregroundStyle(ok ? theme.colors.success : theme.colors.danger)
            Spacer()
            Text(Money.format(ok ? changeMinor : shortMinor, currency))
                .font(.money(15, .heavy)).foregroundStyle(ok ? theme.colors.success : theme.colors.danger)
                .contentTransition(.numericText())
        }
        .padding(.horizontal, Space.md).padding(.vertical, 10)
        .background(ok ? theme.colors.successBg : theme.colors.dangerBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}

// MARK: - Tip

private struct TipCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Binding var tipMinor: Int64
    @Binding var tipMethod: String?
    let selectedMethod: String?
    let currency: String

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack(spacing: 6) {
                MadarIcon("heart.circle", size: 13)
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
                                if active { MadarIcon("checkmark", size: 9) }
                                Text(m.name).font(.ui(11, .semibold))
                            }
                            .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textSecondary)
                            .padding(.horizontal, 11).padding(.vertical, 6)
                            .background(active ? Color(hex: m.color) : theme.colors.surfaceAlt)
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
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

// MARK: - Discount

private struct DiscountSection: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private func discountLabel(_ d: DiscountView) -> String {
        d.dtype == "percentage" ? "\(d.name) \(d.value)%" : d.name
    }

    var body: some View {
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

    private func chip(_ label: String, active: Bool, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: 5) {
                if active { MadarIcon("checkmark", size: 10) }
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

// MARK: - Customer

private struct CustomerSection: View {
    @Environment(\.localize) private var t
    @Binding var customerName: String
    @Binding var notes: String

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionLabel(t("order.customer"))
            MadarTextField(placeholder: t("order.customer_hint"), text: $customerName, icon: "person", caps: .words)
            MadarTextField(placeholder: t("order.notes_hint"), text: $notes, icon: "text.bubble", caps: .words)
        }
    }
}

// MARK: - Shared small builder

/// Small uppercase muted section heading. Free function so every Tender subview
/// renders an identical label without re-declaring it.
@MainActor private func sectionLabel(_ s: String) -> some View {
    SectionLabel(text: s)
}

private struct SectionLabel: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        Text(text).font(.ui(12, .bold)).foregroundStyle(theme.colors.textMuted)
            .tracking(0.6).textCase(.uppercase)
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
                MadarIcon(PayChip.symbol(method.icon), size: IconSize.md)
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(active ? theme.colors.textOnAccent : color)
                Text(method.name).font(.ui(13, .semibold)).lineLimit(1)
                    .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
                Spacer(minLength: 0)
                if active { MadarIcon("checkmark", size: 12)
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
    // Mirrors the Flutter `PaymentMethodX.uiIcon` (lib/core/models/payment_method.dart):
    // the backend stores the method's icon as one of these keys; map each to the
    // shared Lucide icon. Aliases (cash/card/qr/…) kept for older data.
    static func symbol(_ icon: String) -> String {
        switch icon.lowercased() {
        case "money", "cash", "banknote": return "banknote"
        case "credit_card", "card", "creditcard", "visa", "mastercard", "debit": return "creditcard"
        case "wallet", "ewallet", "e_wallet": return "wallet"
        case "pie_chart": return "chart.pie"
        case "delivery": return "bicycle"
        case "qr_code", "qr": return "qrcode"
        case "bank", "transfer", "bank_transfer": return "bank"
        case "gift_card": return "gift"
        case "smartphone", "phone", "mobile", "vodafone", "instapay": return "iphone"
        case "receipt": return "receipt"
        case "store": return "storefront"
        case "star": return "star"
        case "link": return "link"
        default: return "banknote"
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
                MadarIcon(receipt.queuedOffline ? "clock.badge.checkmark" : "checkmark.circle.fill", size: 38)
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
                ReceiptPaper(receipt: receipt, storeName: app.branchName, currency: currency,
                             dateText: app.fmtReceipt(receipt.createdAt), orgLogoUrl: app.orgLogoUrl)
                    .padding(.horizontal, Space.lg)
                    .padding(.bottom, Space.lg)
            }

            // Actions.
            VStack(spacing: Space.sm) {
                printControl
                MadarButton(label: t("order.new_order"), icon: "plus", variant: .outline) { onDone() }
            }
            .padding(Space.lg)
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
    }

    /// The receipt is auto-printed on checkout. Show the print status, and keep a
    /// Reprint button available (a reprint does NOT re-pop the cash drawer).
    @ViewBuilder private var printControl: some View {
        VStack(spacing: Space.sm) {
            switch app.printState {
            case .printed:
                StatusChip(label: t("receipt.printed"), icon: "checkmark.circle", tone: .success)
                    .frame(maxWidth: .infinity)
            case .noPrinter:
                StatusChip(label: t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning)
                    .frame(maxWidth: .infinity)
            case .failed:
                StatusChip(label: t("receipt.print_failed"), icon: "exclamationmark.triangle", tone: .danger)
                    .frame(maxWidth: .infinity)
            default:
                EmptyView()
            }
            MadarButton(
                label: t("receipt.reprint"),
                icon: "printer",
                variant: .outline,
                loading: app.printState == .printing
            ) {
                Task { await app.printCurrentReceipt(kickDrawer: false) }
            }
        }
    }
}
