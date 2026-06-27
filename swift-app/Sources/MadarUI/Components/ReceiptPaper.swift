// On-screen receipt preview — a white "thermal paper" card rendered from the
// core's ReceiptView, so the teller can see exactly what will print BEFORE
// sending it to the printer. Mirrors the ESC/POS layout in receipt.rs
// (layout()): store header, order meta, optional delivery block, item lines with
// modifier / bundle breakdown, totals, footer.
//
// Theme-invariant on purpose: a receipt is always white paper with dark ink,
// regardless of the app's light/dark theme — so it reads like real paper.
import SwiftUI

struct ReceiptPaper: View {
    let receipt: ReceiptView
    let storeName: String
    let currency: String
    /// The order's created-at, pre-formatted in the BRANCH's timezone by the parent
    /// (via `app.fmtReceipt(receipt.createdAt)`) — this component has no AppModel, so
    /// the branch-tz formatting happens upstream and is threaded in as a string.
    let dateText: String
    /// The org's logo URL (from the branch). Rendered at the top of the paper,
    /// mirroring Flutter's `_buildLogoAndBranch`. `nil`/unreachable → just the
    /// branch name (the receipt still reads correctly without the brand mark).
    var orgLogoUrl: String? = nil

    // Fixed paper palette (not theme-driven).
    private let paper = Color(white: 1.0)
    private let ink = Color(white: 0.10)
    private let faint = Color(white: 0.42)
    private let rule = Color(white: 0.80)

    private func money(_ m: Int64) -> String { Money.format(m, currency) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            header
            divider
            meta
            divider
            if receipt.isDelivery { deliveryBlock; divider }
            ForEach(Array(receipt.lines.enumerated()), id: \.offset) { _, line in
                lineBlock(line)
            }
            divider
            totals
            divider
            footer
        }
        .font(.ui(12, .regular))
        .foregroundStyle(ink)
        .padding(18)
        .frame(maxWidth: 360)
        .background(paper)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(rule, lineWidth: 1))
        .shadow(color: .black.opacity(0.12), radius: 10, y: 3)
    }

    // MARK: header
    private var header: some View {
        VStack(spacing: 3) {
            if let logo = orgLogoUrl, !logo.isEmpty, let url = URL(string: logo) {
                // Aspect-preserved (contain), never cropped/squished — a wide
                // wordmark or a square mark both render naturally, mirroring
                // Flutter's `_buildLogoAndBranch`.
                CachedAsyncImage(url: url, contentMode: .fit)
                    .frame(maxWidth: 220, maxHeight: 64)
                    .padding(.top, 2)
                    .padding(.bottom, 6)
            }
            if receipt.isVoided {
                Text("*** VOIDED ***")
                    .font(.ui(13, .bold))
                    .foregroundStyle(Color(red: 0.72, green: 0.10, blue: 0.10))
            }
            Text(storeName.isEmpty ? "MADAR" : storeName.uppercased())
                .font(.ui(15, .bold))
            if receipt.isDelivery, let ch = receipt.deliveryChannel {
                Text("— \(ch == "in_mall" ? "IN-MALL" : "DELIVERY") —")
                    .font(.ui(11, .regular)).foregroundStyle(faint)
            }
        }
        .frame(maxWidth: .infinity, alignment: .center)
    }

    // MARK: order meta (id / number + datetime, optional ref)
    private var meta: some View {
        VStack(spacing: 2) {
            row(orderTitle, dateText)
            if let r = receipt.orderRef { row("Ref: \(r)", "") }
        }
    }

    private var orderTitle: String {
        if let n = receipt.orderNumber { return "Order #\(n)" }
        let seg = receipt.localOrderId.split(separator: "-").first.map(String.init) ?? receipt.localOrderId
        return "Order \(seg.uppercased())"
    }

    // MARK: delivery block
    private var deliveryBlock: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let v = receipt.customerName { row("Customer", v) }
            if let v = receipt.customerPhone { row("Phone", v) }
            if let v = receipt.deliveryAddress {
                Text("Addr: \(v)").frame(maxWidth: .infinity, alignment: .leading)
            }
            if let v = receipt.deliveryZone { row("Zone", v) }
        }
    }

    // MARK: one item line + modifier / bundle breakdown
    @ViewBuilder private func lineBlock(_ line: ReceiptLineView) -> some View {
        let name = Self.nameWithSize(line.name, line.sizeLabel)
        row("\(line.qty)× \(name)", money(line.lineTotalMinor))
        if line.isBundle {
            ForEach(Array(line.components.enumerated()), id: \.offset) { _, c in
                Text("  – \(Self.nameWithSize(c.name, c.sizeLabel))").foregroundStyle(faint)
                ForEach(Array(c.addons.enumerated()), id: \.offset) { _, a in modifierRow("    + ", a) }
                ForEach(Array(c.optionals.enumerated()), id: \.offset) { _, o in modifierRow("    + ", o) }
            }
        } else {
            ForEach(Array(line.addons.enumerated()), id: \.offset) { _, a in modifierRow("  + ", a) }
            ForEach(Array(line.optionals.enumerated()), id: \.offset) { _, o in modifierRow("  + ", o) }
        }
    }

    @ViewBuilder private func modifierRow(_ prefix: String, _ m: ReceiptModifierView) -> some View {
        row("\(prefix)\(m.name)", m.priceMinor > 0 ? "+\(money(m.priceMinor))" : "", faint: true)
    }

    // MARK: totals
    private var totals: some View {
        VStack(spacing: 2) {
            row("Subtotal", money(receipt.subtotalMinor))
            if receipt.discountMinor > 0 { row("Discount", "−\(money(receipt.discountMinor))") }
            if receipt.taxMinor > 0 { row("Tax", money(receipt.taxMinor)) }
            if receipt.deliveryFeeMinor > 0 { row("Delivery", money(receipt.deliveryFeeMinor)) }
            row("TOTAL", money(receipt.totalMinor), bold: true)
            if receipt.tipMinor > 0 { row("Tip", money(receipt.tipMinor)) }
            if receipt.isCash {
                row("Cash", money(receipt.amountTenderedMinor))
                row("Change", money(receipt.changeMinor))
            }
        }
    }

    // MARK: footer
    private var footer: some View {
        VStack(spacing: 2) {
            Text(receipt.paymentLabel.uppercased())
                .font(.ui(11, .semibold))
            if let teller = receipt.tellerName { Text("Served by \(teller)").foregroundStyle(faint) }
            Text("Thank you!").padding(.top, 2)
        }
        .frame(maxWidth: .infinity, alignment: .center)
    }

    // MARK: helpers
    private var divider: some View {
        Rectangle().fill(rule).frame(height: 1).padding(.vertical, 1)
    }

    private func row(_ left: String, _ right: String, bold: Bool = false, faint isFaint: Bool = false) -> some View {
        HStack(alignment: .top, spacing: 6) {
            Text(left)
                .font(.ui(bold ? 13 : 12, bold ? .bold : .regular))
            Spacer(minLength: 4)
            if !right.isEmpty {
                Text(right)
                    .font(.ui(bold ? 13 : 12, bold ? .bold : .regular))
            }
        }
        .foregroundStyle(isFaint ? faint : ink)
    }

    static func nameWithSize(_ base: String, _ size: String?) -> String {
        if let s = size, !s.isEmpty { return "\(base) (\(s))" }
        return base
    }
}

// A receipt is uniquely identified by its order id — lets it drive `.madarSheet(item:)`.
extension ReceiptView: Identifiable {
    public var id: String { localOrderId }
}

/// A sheet that previews a receipt (ReceiptPaper) with a Print action — used to
/// preview a past order before reprinting it.
struct ReceiptPreviewSheet: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let receipt: ReceiptView
    let onClose: () -> Void
    @State private var printing = false

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(t("receipt.title")).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                Button { onClose() } label: {
                    MadarIcon("xmark", size: 15)
                        .foregroundStyle(theme.colors.textMuted).frame(width: 32, height: 32)
                        .background(theme.colors.surfaceAlt).clipShape(Circle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, Space.lg).padding(.bottom, Space.md)

            ScrollView {
                ReceiptPaper(receipt: receipt, storeName: app.branchName, currency: currency,
                             dateText: app.fmtReceipt(receipt.createdAt), orgLogoUrl: app.orgLogoUrl)
                    .padding(.horizontal, Space.lg).padding(.bottom, Space.lg)
            }

            VStack {
                MadarButton(label: t("receipt.print"), icon: "printer", loading: printing) {
                    printing = true
                    Task { await app.printReceiptView(receipt); printing = false }
                }
            }
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
    }
}
