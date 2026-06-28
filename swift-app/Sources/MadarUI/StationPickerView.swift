// Kitchen-display commissioning — the screen a `kitchen`-role device shows once
// it's bound to a branch but has no station yet (the core routes here via
// `.deviceSetup`; without it a KDS device dead-ended). Pick a station → the core
// pins it (set_device_station) → the route recomputes to the KitchenDisplay.
// Mirrors the Login / OpenShift brand-panel split. Mirror of StationPickerScreen.kt.
import SwiftUI

struct StationPickerView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var loading = true

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= Responsive.wide
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        BrandPanel().frame(width: geo.size.width * 0.5)
                        column(showLogo: false).frame(maxWidth: .infinity)
                    }
                } else {
                    column(showLogo: true)
                }
            }
        }
        .task {
            app.clearError()
            await app.loadKdsStations()
            loading = false
        }
    }

    private func column(showLogo: Bool) -> some View {
        ScrollView {
            VStack(spacing: Space.lg) {
                if showLogo { MadarMark(size: 56) }

                VStack(spacing: Space.sm) {
                    ZStack {
                        Circle().fill(theme.colors.accentBg).frame(width: 56, height: 56)
                        MadarIcon("flame.fill", size: 28).foregroundStyle(theme.colors.accent)
                    }
                    Text(t("setup.choose_station")).font(.ui(26, .heavy))
                        .foregroundStyle(theme.colors.textPrimary).multilineTextAlignment(.center)
                    Text(t("setup.choose_station_desc")).font(.ui(13))
                        .foregroundStyle(theme.colors.textMuted).multilineTextAlignment(.center)
                    if !app.branchName.isEmpty {
                        StatusChip(label: app.branchName, icon: "building.2", tone: .info).padding(.top, Space.xs)
                    }
                }

                if let err = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: err, tone: .danger)
                }

                if loading {
                    ProgressView().controlSize(.large).tint(theme.colors.accent).padding(Space.xl)
                } else if app.kdsStations.isEmpty {
                    Text(t("setup.no_stations")).font(.ui(13))
                        .foregroundStyle(theme.colors.textMuted).multilineTextAlignment(.center)
                } else {
                    VStack(spacing: Space.sm) {
                        ForEach(app.kdsStations, id: \.id) { st in stationCard(st) }
                    }
                }

                MadarButton(label: t("home.sign_out"), icon: "rectangle.portrait.and.arrow.right", variant: .ghost) {
                    app.signOut()
                }
                .padding(.top, Space.sm)
            }
            .frame(maxWidth: 480)
            .padding(.horizontal, Space.xxl).padding(.vertical, 48)
            .frame(maxWidth: .infinity)
        }
    }

    private func stationCard(_ st: KdsStationView) -> some View {
        Button { app.setDeviceStation(st.id) } label: {
            HStack(spacing: Space.md) {
                ZStack {
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .fill(theme.colors.accentBg).frame(width: 40, height: 40)
                    MadarIcon("flame.fill", size: IconSize.lg).foregroundStyle(theme.colors.accent)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text(st.name).font(.ui(16, .bold)).foregroundStyle(theme.colors.textPrimary)
                    if st.isDefault {
                        Text(t("setup.station_default")).font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted)
                    }
                }
                Spacer()
                MadarIcon("chevron.forward", size: IconSize.md).foregroundStyle(theme.colors.textMuted)
            }
            .padding(Space.lg)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1))
            .elevation(.card)
        }
        .buttonStyle(.pressable)
    }
}
