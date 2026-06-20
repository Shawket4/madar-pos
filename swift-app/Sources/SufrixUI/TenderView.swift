// Tender — the checkout sheet. Pick a payment method, take cash (with live
// change), and place the order through the core (online or queued offline). On
// success the same sheet flips to a receipt confirmation. All money + the order
// assembly live in the core; this view only collects the tender and renders.
import SwiftUI

struct TenderView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var selectedMethod: String?
    @State private var tenderedMinor: Int64 = 0

    private var currency: String { app.session?.currencyCode ?? "" }
    private var method: PaymentMethodView? { app.paymentMethods.first { $0.id == selectedMethod } }
    private var isCash: Bool { method?.isCash ?? false }
    private var total: Int64 { app.cartTotals.totalMinor }
    private var changeMinor: Int64 { max(0, tenderedMinor - total) }
    private var canPlace: Bool {
        selectedMethod != nil && !app.isPlacingOrder && (!isCash || tenderedMinor >= total)
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

    private var tenderForm: some View {
        ScrollView {
            VStack(spacing: Space.xl) {
                HStack {
                    Text(t("order.tender")).font(.ui(22, .heavy)).foregroundStyle(theme.colors.textPrimary)
                    Spacer()
                    Button { onClose() } label: {
                        Image(systemName: "xmark").font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(theme.colors.textMuted)
                    }
                    .buttonStyle(.plain)
                }

                VStack(alignment: .leading, spacing: Space.sm) {
                    Text(t("order.payment_method"))
                        .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), spacing: Space.sm)], spacing: Space.sm) {
                        ForEach(app.paymentMethods, id: \.id) { m in
                            MethodChip(label: m.name, active: m.id == selectedMethod) {
                                selectedMethod = m.id
                            }
                        }
                    }
                }

                let activeDiscounts = app.discounts.filter { $0.isActive }
                if !activeDiscounts.isEmpty {
                    VStack(alignment: .leading, spacing: Space.sm) {
                        Text(t("order.discount"))
                            .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), spacing: Space.sm)], spacing: Space.sm) {
                            MethodChip(label: t("order.no_discount"), active: app.cartDiscountId == nil) {
                                app.setDiscount(nil)
                            }
                            ForEach(activeDiscounts, id: \.id) { d in
                                MethodChip(label: discountLabel(d), active: app.cartDiscountId == d.id) {
                                    app.setDiscount(d.id)
                                }
                            }
                        }
                    }
                }

                if isCash {
                    VStack(alignment: .leading, spacing: Space.sm) {
                        Text(t("order.cash_received"))
                            .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                        AmountField(amountMinor: $tenderedMinor, currencyCode: currency)
                    }
                }

                VStack(spacing: Space.sm) {
                    if app.cartTotals.discountMinor > 0 {
                        HStack {
                            Text(t("order.discount")).font(.ui(14, .medium)).foregroundStyle(theme.colors.success)
                            Spacer()
                            Text("−\(Money.format(app.cartTotals.discountMinor, currency))")
                                .font(.money(14, .semibold)).foregroundStyle(theme.colors.success)
                        }
                    }
                    SummaryRow(label: t("order.total"), value: Money.format(total, currency), emphasized: true)
                    if isCash {
                        SummaryRow(label: t("order.change"), value: Money.format(changeMinor, currency))
                    }
                }

                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                }

                SufrixButton(label: t("order.place_order"), icon: "checkmark", loading: app.isPlacingOrder) {
                    guard let id = selectedMethod else { return }
                    Task { await app.placeOrder(paymentMethodId: id, amountTenderedMinor: isCash ? tenderedMinor : 0) }
                }
                .opacity(canPlace ? 1 : 0.5)
                .allowsHitTesting(canPlace)
            }
            .frame(maxWidth: 460)
            .frame(maxWidth: .infinity)
            .padding(Space.xl)
        }
    }
}

private struct MethodChip: View {
    @Environment(\.theme) private var theme
    let label: String
    let active: Bool
    let action: () -> Void

    var body: some View {
        Button { Haptics.selection(); action() } label: {
            Text(label)
                .font(.ui(14, .semibold))
                .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(active ? theme.colors.accent : theme.colors.surface)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }
}

private struct ReceiptConfirmation: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let receipt: ReceiptView
    let currency: String
    let onDone: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: Space.lg) {
                SufrixMark(size: 52)
                Text(t("order.order_placed")).font(.ui(22, .heavy)).foregroundStyle(theme.colors.textPrimary)
                StatusChip(
                    label: t(receipt.queuedOffline ? "order.queued_hint" : "order.sent_hint"),
                    icon: receipt.queuedOffline ? "clock" : "checkmark.circle",
                    tone: receipt.queuedOffline ? .warning : .success
                )

                VStack(spacing: Space.sm) {
                    ForEach(Array(receipt.lines.enumerated()), id: \.offset) { _, line in
                        HStack {
                            Text("\(line.qty)× \(line.name)")
                                .font(.ui(14, .medium)).foregroundStyle(theme.colors.textSecondary)
                            Spacer()
                            Text(Money.format(line.lineTotalMinor, currency))
                                .font(.money(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                        }
                    }
                }
                .padding(Space.lg)
                .background(theme.colors.surface)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                        .strokeBorder(theme.colors.border, lineWidth: 1)
                )

                VStack(spacing: Space.sm) {
                    SummaryRow(label: t("order.subtotal"), value: Money.format(receipt.subtotalMinor, currency))
                    if receipt.discountMinor > 0 {
                        SummaryRow(label: t("order.discount"), value: "−\(Money.format(receipt.discountMinor, currency))")
                    }
                    SummaryRow(label: t("order.tax"), value: Money.format(receipt.taxMinor, currency))
                    SummaryRow(label: t("order.total"), value: Money.format(receipt.totalMinor, currency), emphasized: true)
                    if receipt.isCash {
                        SummaryRow(label: t("order.cash_received"), value: Money.format(receipt.amountTenderedMinor, currency))
                        SummaryRow(label: t("order.change"), value: Money.format(receipt.changeMinor, currency))
                    }
                }

                printControl

                SufrixButton(label: t("order.new_order"), icon: "plus") { onDone() }
                    .padding(.top, Space.sm)
            }
            .frame(maxWidth: 460)
            .frame(maxWidth: .infinity)
            .padding(Space.xl)
        }
    }

    /// Print receipt — best-effort send to the configured network printer, with
    /// inline state (printing / sent / unreachable / not-configured).
    @ViewBuilder private var printControl: some View {
        switch app.printState {
        case .printed:
            StatusChip(label: t("receipt.printed"), icon: "checkmark.circle", tone: .success)
        case .noPrinter:
            StatusChip(label: t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning)
        default:
            SufrixButton(
                label: app.printState == .failed ? t("receipt.print_failed") : t("receipt.print"),
                icon: "printer",
                variant: .outline,
                loading: app.printState == .printing
            ) {
                Task { await app.printCurrentReceipt() }
            }
        }
    }
}

/// A label/value row shared by the tender form + receipt.
private struct SummaryRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var emphasized = false

    var body: some View {
        HStack {
            Text(label)
                .font(.ui(emphasized ? 16 : 14, emphasized ? .bold : .medium))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : theme.colors.textSecondary)
            Spacer()
            Text(value)
                .font(.money(emphasized ? 18 : 14, emphasized ? .heavy : .semibold))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : theme.colors.textSecondary)
        }
    }
}
