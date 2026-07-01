// Floor plan + host board (POS render-and-operate surface). Geometry is authored
// in the dashboard; this screen renders the branch floor to scale from the core's
// `FloorTableView` geometry, shows live table status, and drives host operations
// (set status, seat a booking, move a ticket). ALL logic is in the core — this is
// pure SwiftUI over `AppModel`'s `@Published` mirror.
import SwiftUI

extension FloorTableView: Identifiable {}
extension FloorSectionView: Identifiable {}
extension ReservationView: Identifiable {}

private func tableColor(_ status: String) -> Color {
    switch status {
    case "free": return .green
    case "held": return .orange
    case "seated": return .blue
    case "dirty": return .gray
    default: return .secondary
    }
}

struct FloorPlanView: View {
    @ObservedObject var app: AppModel
    @State private var activeSection: String?
    @State private var statusTarget: FloorTableView?
    @State private var seatTarget: ReservationView?

    private var sectionId: String? { activeSection ?? app.floorSections.first?.id }
    private var section: FloorSectionView? { app.floorSections.first { $0.id == sectionId } }
    private var tables: [FloorTableView] { app.floorTables.filter { $0.sectionId == sectionId } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if app.floorSections.count > 1 {
                    Picker("", selection: Binding(get: { sectionId ?? "" }, set: { activeSection = $0 })) {
                        ForEach(app.floorSections) { Text($0.name).tag($0.id) }
                    }
                    .pickerStyle(.segmented)
                }

                floorCanvas
                reservationsList
            }
            .padding()
        }
        .task { await app.loadFloor() }
        .refreshable { await app.loadFloor() }
        .confirmationDialog(app.t("reservations.setStatus"), isPresented: Binding(get: { statusTarget != nil }, set: { if !$0 { statusTarget = nil } })) {
            ForEach(["free", "held", "seated", "dirty"], id: \.self) { s in
                Button(app.t("reservations.status_\(s)")) {
                    if let tb = statusTarget { Task { await app.setTableStatus(tb.id, status: s) } }
                    statusTarget = nil
                }
            }
            Button(app.t("common.cancel"), role: .cancel) { statusTarget = nil }
        }
        .sheet(item: $seatTarget) { booking in
            SeatSheet(app: app, booking: booking, tables: tables)
        }
    }

    private var floorCanvas: some View {
        let w = CGFloat(section?.canvasW ?? 1000)
        let h = CGFloat(section?.canvasH ?? 700)
        return GeometryReader { geo in
            let scale = geo.size.width / w
            ZStack(alignment: .topLeading) {
                ForEach(tables) { tb in
                    let isCircle = tb.shape == "circle"
                    Group {
                        if isCircle {
                            Ellipse().fill(tableColor(tb.status).opacity(0.22)).overlay(Ellipse().stroke(tableColor(tb.status), lineWidth: 2))
                        } else {
                            RoundedRectangle(cornerRadius: 8).fill(tableColor(tb.status).opacity(0.22)).overlay(RoundedRectangle(cornerRadius: 8).stroke(tableColor(tb.status), lineWidth: 2))
                        }
                    }
                    .frame(width: CGFloat(tb.width) * scale, height: CGFloat(tb.height) * scale)
                    .overlay(
                        VStack(spacing: 1) {
                            Text(tb.label).font(.caption).bold()
                            Text("\(tb.seats)").font(.caption2).foregroundStyle(.secondary)
                        }
                    )
                    .position(x: (CGFloat(tb.posX) + CGFloat(tb.width) / 2) * scale,
                              y: (CGFloat(tb.posY) + CGFloat(tb.height) / 2) * scale)
                    .onTapGesture { statusTarget = tb }
                }
            }
            .frame(width: geo.size.width, height: h * scale)
            .background(RoundedRectangle(cornerRadius: 12).fill(Color.secondary.opacity(0.06)))
        }
        .aspectRatio(w / h, contentMode: .fit)
    }

    private var reservationsList: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(app.t("reservations.title")).font(.headline)
            if app.reservations.isEmpty {
                Text(app.t("reservations.noBookings")).foregroundStyle(.secondary).font(.subheadline)
            }
            ForEach(app.reservations) { b in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(b.customerName).font(.subheadline).bold()
                        Text("\(b.partySize) · \(b.status)").font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(app.t("reservations.seat")) { seatTarget = b }.buttonStyle(.bordered)
                    Button { Task { await app.notifyReservation(b.id) } } label: { Image(systemName: "bell.fill") }
                }
                .padding(10)
                .background(RoundedRectangle(cornerRadius: 10).fill(Color.secondary.opacity(0.06)))
            }
        }
    }
}

/// Pick one or more tables to seat a party (multiple ⇒ merged tables).
private struct SeatSheet: View {
    @ObservedObject var app: AppModel
    let booking: ReservationView
    let tables: [FloorTableView]
    @Environment(\.dismiss) private var dismiss
    @State private var picked: Set<String> = []

    var body: some View {
        NavigationStack {
            List(tables) { tb in
                Button {
                    if picked.contains(tb.id) { picked.remove(tb.id) } else { picked.insert(tb.id) }
                } label: {
                    HStack {
                        VStack(alignment: .leading) {
                            Text(tb.label).bold()
                            Text("\(tb.seats) · \(tb.status)").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if picked.contains(tb.id) { Image(systemName: "checkmark.circle.fill").foregroundStyle(.tint) }
                    }
                }
            }
            .navigationTitle(booking.customerName)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(app.t("reservations.seat")) {
                        Task { if await app.seatReservation(booking.id, tableIds: Array(picked)) { dismiss() } }
                    }.disabled(picked.isEmpty)
                }
                ToolbarItem(placement: .cancellationAction) { Button(app.t("common.cancel")) { dismiss() } }
            }
        }
    }
}
