// Kitchen-display commissioning — the screen a `kitchen`-role device shows once
// it's bound to a branch but has no station yet (the core routes here via
// `.deviceSetup`; without it a KDS device dead-ended). Pick a station → the core
// pins it (set_device_station) → the route recomputes to the KitchenDisplay.
// Mirrors the Login / OpenShift brand-panel split — name-first hero, the station
// list on its own bordered surface card. Mirror of StationPickerScreen.kt.
import SwiftUI

struct StationPickerView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme

    @State private var loading = true

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= Responsive.wide
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        BrandPanel().frame(width: geo.size.width * 0.5)
                        StationColumn(app: app, loading: $loading, showLogo: false).frame(maxWidth: .infinity)
                    }
                } else {
                    StationColumn(app: app, loading: $loading, showLogo: true)
                }
            }
        }
        .task {
            app.clearError()
            await app.loadKdsStations()
            loading = false
        }
    }
}

/// The centered commissioning column — hero greeting, the station list on its own
/// bordered surface card, and a recessive sign-out. Its own `View` struct (not a
/// `some View` method) so it owns its environment and recomputes independently.
private struct StationColumn: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Binding var loading: Bool
    let showLogo: Bool

    var body: some View {
        ScrollView {
            VStack(spacing: Space.lg) {
                if showLogo { MadarMark(size: 56) }

                // ── Hero greeting (the commissioning prompt IS the hero) ───────
                StationGreeting(app: app)

                if let err = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: err, tone: .danger)
                }

                // ── Station list on its own bordered surface card (matches the
                // Order / OpenShift raised, hairline-bordered surfaces) ────────
                MadarCard(spacing: Space.md) {
                    SectionHeader(text: t("setup.title"), icon: "square.stack.3d.up.fill")
                    if loading {
                        ProgressView().controlSize(.large).tint(theme.colors.accent)
                            .frame(maxWidth: .infinity).padding(Space.xl)
                    } else if app.kdsStations.isEmpty {
                        Text(t("setup.no_stations")).font(.ui(13))
                            .foregroundStyle(theme.colors.textMuted)
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: .infinity).padding(.vertical, Space.md)
                    } else {
                        ForEach(app.kdsStations, id: \.id) { st in
                            StationCard(station: st) { app.setDeviceStation(st.id) }
                        }
                    }
                }

                // ── Recessive exit ─────────────────────────────────────────────
                MadarButton(label: t("home.sign_out"), icon: "rectangle.portrait.and.arrow.right", variant: .ghost) {
                    app.signOut()
                }
            }
            .frame(maxWidth: 480)
            .padding(.horizontal, Space.xxl).padding(.vertical, 48)
            .frame(maxWidth: .infinity)
        }
    }
}

/// The commissioning hero — accent-tinted station tile, bold title, supporting line,
/// and the bound branch as an info chip. Mirrors the OpenShift greeting.
private struct StationGreeting: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        VStack(spacing: Space.sm) {
            ZStack {
                Circle().fill(theme.colors.accentBg).frame(width: 56, height: 56)
                MadarIcon("fork.knife", size: 28).foregroundStyle(theme.colors.accent)
            }
            Text(t("setup.choose_station")).font(.ui(26, .heavy))
                .foregroundStyle(theme.colors.textPrimary).multilineTextAlignment(.center)
            Text(t("setup.choose_station_desc")).font(.ui(13))
                .foregroundStyle(theme.colors.textMuted).multilineTextAlignment(.center)
            if !app.branchName.isEmpty {
                StatusChip(label: app.branchName, icon: "building.2", tone: .info).padding(.top, Space.xs)
            }
        }
    }
}

/// One selectable station — leading tone-tile + name, default flagged with an accent
/// StatusChip and lifted with a heavier accent border + filled tile (mirrors the
/// Kitchen "ready" card's accent emphasis), trailing chevron. Fixed row height so
/// every station aligns. Its own `View` struct so the tap action lives outside any
/// parent `body`.
private struct StationCard: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let station: KdsStationView
    let onSelect: () -> Void

    var body: some View {
        Button(action: onSelect) {
            HStack(spacing: Space.md) {
                // Default station gets a FILLED accent tile so it reads as "this one"
                // at a glance; the rest stay tinted. Same glyph, different weight.
                ZStack {
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .fill(station.isDefault ? theme.colors.accent : theme.colors.accentBg)
                        .frame(width: 44, height: 44)
                    MadarIcon("fork.knife", size: IconSize.xl)
                        .foregroundStyle(station.isDefault ? theme.colors.textOnAccent : theme.colors.accent)
                }
                Text(station.name).font(.ui(17, .bold)).foregroundStyle(theme.colors.textPrimary)
                Spacer(minLength: Space.sm)
                if station.isDefault { StatusChip(label: t("setup.station_default"), tone: .accent) }
                MadarIcon("chevron.forward", size: IconSize.md).foregroundStyle(theme.colors.textMuted)
            }
            .padding(.horizontal, Space.lg)
            .frame(maxWidth: .infinity, minHeight: 72, alignment: .leading)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(
                    station.isDefault ? theme.colors.accent.opacity(0.55) : theme.colors.borderLight,
                    lineWidth: station.isDefault ? 2 : 1))
            .elevation(.card)
        }
        .buttonStyle(.pressable)
    }
}
