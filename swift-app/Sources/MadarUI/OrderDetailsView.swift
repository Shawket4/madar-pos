// Order details — the ONE shared "what's actually in this order" layout, used by
// both Orders tabs. Before this, the teller could see a ticket/delivery card's
// total but never its contents; now a tap opens a details sheet that renders the
// real line items (qty × name, size/modifiers, per-line price), the money
// breakdown (subtotal / discount / delivery fee / total), and the order's context
// (customer, table/covers for tickets; customer/phone/address/channel/notes for
// delivery).
//
// Both tickets and delivery carry their real priced lines (`TicketView.lines` /
// `DeliveryOrderView.lines`, the latter projected from the frozen `cart.lines`
// snapshot), so both bodies list every line through the same `OrderLinesCard`.
//
// Layout primitives here (`OrderContextRow`, `OrderLineRow`, `OrderMoneyRow`) are
// shared so both bodies read in one visual language (P3).
import SwiftUI

// MARK: - Ticket details

/// The full contents of an open ticket — context header, its frozen lines, then
/// the subtotal/total. Pure presentation; the owner supplies the sheet chrome.
struct TicketDetailsView: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let ticket: TicketView
    let currency: String

    private var customerName: String? {
        guard let n = ticket.customerName, !n.isEmpty else { return nil }
        return n
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.lg) {
            OrderDetailsHeader(
                title: ticket.ticketRef ?? t("waiter.ticket"),
                trailingChip: { TicketStatusChip(status: ticket.status) })

            // Context — covering customer + table/covers.
            OrderContextCard {
                if let name = customerName {
                    OrderContextRow(icon: "person.fill", label: t("receipt.customer"), value: name)
                }
                if let table = ticket.tableId, !table.isEmpty {
                    OrderContextRow(icon: "square.grid.2x2", label: t("order.table"), value: table)
                }
                if let covers = ticket.guestCount, covers > 0 {
                    OrderContextRow(icon: "person.2.fill", label: t("waiter.covers"), value: "\(covers)")
                }
            }

            // The ticket's frozen line items.
            OrderLinesCard(lines: ticket.lines, currency: currency)

            // Money — a ticket carries just a subtotal that equals its total.
            OrderMoneyCard {
                OrderMoneyRow(label: t("order.subtotal"), value: Money.format(ticket.subtotalMinor, currency))
                OrderTotalRow(label: t("order.total"), value: Money.format(ticket.subtotalMinor, currency))
            }
        }
    }
}

// MARK: - Delivery details

/// A delivery order's fulfillment context + money breakdown. The view model has
/// no per-line data, so the body shows the item-count + the subtotal/discount/
/// fee/total split plus the customer's address and instructions.
struct DeliveryDetailsView: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let order: DeliveryOrderView
    let currency: String

    var body: some View {
        VStack(alignment: .leading, spacing: Space.lg) {
            OrderDetailsHeader(
                title: order.orderRef ?? t("delivery.title"),
                trailingChip: { StatusChip(label: t("delivery.\(order.channel)"), tone: .neutral) })

            OrderContextCard {
                OrderContextRow(icon: "person.fill", label: t("receipt.customer"), value: order.customerName)
                OrderContextRow(icon: "phone.fill", label: t("receipt.phone"), value: order.customerPhone)
                if let addr = order.address, !addr.isEmpty {
                    OrderContextRow(icon: "mappin.circle.fill", label: t("receipt.address"), value: addr)
                }
            }

            // Delivery instructions ("leave at door") — fulfillment-critical.
            if let note = order.deliveryNotes, !note.isEmpty {
                OrderNoteBanner(note: note)
            }

            // The real priced lines from the frozen cart snapshot — same card the
            // tickets use, so both tabs read identically.
            OrderLinesCard(lines: order.lines, currency: currency)

            OrderMoneyCard {
                OrderMoneyRow(label: t("order.subtotal"), value: Money.format(order.subtotalMinor, currency))
                if order.discountMinor > 0 {
                    OrderMoneyRow(label: t("order.discount"), value: "−\(Money.format(order.discountMinor, currency))",
                                  tone: theme.colors.success)
                }
                if order.deliveryFeeMinor > 0 {
                    OrderMoneyRow(label: t("receipt.delivery_fee"), value: Money.format(order.deliveryFeeMinor, currency))
                }
                OrderTotalRow(label: t("order.total"), value: Money.format(order.totalMinor, currency))
            }
        }
    }
}

// MARK: - Shared building blocks

/// A details sheet header — bold title on the leading edge, a status/channel chip
/// on the trailing edge.
private struct OrderDetailsHeader<Chip: View>: View {
    @Environment(\.theme) private var theme
    let title: String
    @ViewBuilder let trailingChip: () -> Chip

    var body: some View {
        HStack(spacing: Space.sm) {
            Text(title).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
            Spacer(minLength: Space.sm)
            trailingChip()
        }
    }
}

/// A bordered surface card that stacks context rows (customer / table / address).
private struct OrderContextCard<Content: View>: View {
    @Environment(\.theme) private var theme
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) { content() }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

/// A single context line — leading accent tone-tile icon, a muted label, then the
/// value (wrapping for long addresses).
struct OrderContextRow: View {
    @Environment(\.theme) private var theme
    let icon: String
    let label: String
    let value: String

    var body: some View {
        HStack(alignment: .top, spacing: Space.sm) {
            MadarIcon(icon, size: IconSize.md)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 32, height: 32)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            VStack(alignment: .leading, spacing: 1) {
                Text(label.uppercased()).font(.ui(10, .bold)).tracking(0.5)
                    .foregroundStyle(theme.colors.textMuted)
                Text(value).font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
    }
}

/// The line-items card — a header, then one row per line. Reused wherever a real
/// line list exists (tickets today; any future order-with-lines).
struct OrderLinesCard: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let lines: [TicketLineView]
    let currency: String

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            SectionHeader(text: t("order.items"), icon: "bag")
            if lines.isEmpty {
                Text(t("delivery.empty")).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            } else {
                VStack(spacing: Space.sm) {
                    ForEach(lines.indices, id: \.self) { i in
                        OrderLineRow(line: lines[i], currency: currency)
                        if i < lines.count - 1 {
                            Rectangle().fill(theme.colors.border).frame(height: 1)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

/// One order line — a bold qty badge, the item name (+ size / modifiers stacked
/// beneath), and the per-line total on the trailing edge. Voided lines strike out.
struct OrderLineRow: View {
    @Environment(\.theme) private var theme
    let line: TicketLineView
    let currency: String

    /// Size label + any modifiers, joined into one muted sub-line.
    private var detail: String? {
        var parts: [String] = []
        if let size = line.sizeLabel, !size.isEmpty { parts.append(size) }
        parts.append(contentsOf: line.modifiers)
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(alignment: .top, spacing: Space.sm) {
            // Qty badge — a small accent-tinted pill.
            Text("\(line.qty)×")
                .font(.money(13, .heavy)).foregroundStyle(theme.colors.accent)
                .padding(.horizontal, 7).padding(.vertical, 3)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(line.name).font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    .strikethrough(line.voided)
                    .fixedSize(horizontal: false, vertical: true)
                if let detail {
                    Text(detail).font(.ui(11, .medium)).foregroundStyle(theme.colors.textMuted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: Space.sm)
            Text(Money.format(line.lineTotalMinor, currency))
                .font(.money(14, .bold))
                .foregroundStyle(line.voided ? theme.colors.textMuted : theme.colors.textPrimary)
                .strikethrough(line.voided)
        }
    }
}

/// The money-breakdown card — subtotal/discount/fee rows then a tinted-teal total.
struct OrderMoneyCard<Content: View>: View {
    @Environment(\.theme) private var theme
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(spacing: Space.xs) { content() }
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

/// A light money sub-row (subtotal / discount / delivery fee).
struct OrderMoneyRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var tone: Color? = nil

    var body: some View {
        HStack {
            Text(label).font(.ui(13, .medium)).foregroundStyle(tone ?? theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(tone ?? theme.colors.textSecondary)
        }
    }
}

/// The grand-total row — the tinted-teal hero block (matches the checkout drawer).
struct OrderTotalRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label).font(.ui(14, .bold)).foregroundStyle(theme.colors.accent)
            Spacer()
            Text(value).font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.md)
        .background(theme.colors.accentBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .padding(.top, Space.xs)
    }
}

/// A warning-tinted instruction banner (delivery notes).
private struct OrderNoteBanner: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let note: String

    var body: some View {
        HStack(alignment: .top, spacing: Space.sm) {
            MadarIcon("text.bubble.fill", size: IconSize.md).foregroundStyle(theme.colors.warning)
            VStack(alignment: .leading, spacing: 1) {
                Text(t("order.notes").uppercased()).font(.ui(10, .bold)).tracking(0.5)
                    .foregroundStyle(theme.colors.warning)
                Text(note).font(.ui(13, .medium)).foregroundStyle(theme.colors.warning)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(Space.md)
        .background(theme.colors.warningBg)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.warning.opacity(Opacity.border), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}
