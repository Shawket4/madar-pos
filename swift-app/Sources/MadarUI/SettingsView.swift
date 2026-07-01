// Settings — appearance (theme), language (en/ar, live), device reconfigure,
// diagnostics, sign out. Reachable from the order action bar. Theme + locale are
// persisted by AppModel; the locale change re-resolves strings + RTL via the core.
import SwiftUI

struct SettingsView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                SettingsHeader(onClose: onClose)
                ScrollView {
                    VStack(spacing: Space.lg) {
                        if let error = app.errorMessage {
                            NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning)
                        }
                        AccountCard(app: app)
                        appearanceCard
                        languageCard
                        PrinterCard(app: app)
                        TillCard(app: app)
                        LanCard(app: app)
                        deviceCard
                        DiagnosticsCard(app: app)
                        MadarButton(label: t("settings.sign_out"),
                                     icon: "rectangle.portrait.and.arrow.right", variant: .danger,
                                     action: signOut)
                    }
                    .frame(maxWidth: 640)
                    .frame(maxWidth: .infinity)
                    .padding(Space.lg)
                }
            }
        }
        .onAppear { app.clearError() }
    }

    // MARK: - Actions (kept out of `body`)

    private func signOut() {
        // Sign-out (→ login) requires a closed drawer first.
        if app.hasOpenShift {
            app.flagError(t("settings.sign_out_shift_open"))
        } else {
            app.signOut()
            onClose()
        }
    }

    private func reconfigure() {
        Haptics.selection()
        // Reconfiguring re-provisions the device — only with a closed drawer.
        if app.hasOpenShift {
            app.flagError(t("settings.reconfigure_shift_open"))
        } else {
            app.beginReconfigure()
            onClose()
        }
    }

    // MARK: - Inline cards (no per-card state)

    private var appearanceCard: some View {
        SettingsCard(t("settings.appearance")) {
            HStack(spacing: Space.sm) {
                SettingsChip(t("settings.theme_light"), app.themeMode == .light) { app.themeMode = .light }
                SettingsChip(t("settings.theme_dark"), app.themeMode == .dark) { app.themeMode = .dark }
                SettingsChip(t("settings.theme_system"), app.themeMode == .system) { app.themeMode = .system }
            }
        }
    }

    private var languageCard: some View {
        SettingsCard(t("settings.language")) {
            HStack(spacing: Space.sm) {
                SettingsChip("English", app.locale.hasPrefix("en")) { app.locale = "en" }
                SettingsChip("العربية", app.locale.hasPrefix("ar")) { app.locale = "ar" }
            }
        }
    }

    private var deviceCard: some View {
        SettingsCard(t("settings.device")) {
            Button(action: reconfigure) {
                HStack(spacing: Space.lg) {
                    MadarIcon("building.2", size: 20).foregroundStyle(theme.colors.textSecondary)
                    Text(t("settings.reconfigure")).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    Spacer()
                    MadarIcon("chevron.forward", size: 16).foregroundStyle(theme.colors.textMuted)
                }
            }
            .buttonStyle(.pressable)
        }
    }
}

// MARK: - Header

private struct SettingsHeader: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            Text(t("settings.title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

// MARK: - Account

private struct AccountCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        SettingsCard(t("settings.account")) {
            HStack(spacing: Space.md) {
                ZStack {
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .fill(theme.colors.navyBg).frame(width: 48, height: 48)
                    Text(String((app.shift?.tellerName ?? "?").prefix(1)).uppercased())
                        .font(.ui(16, .bold)).foregroundStyle(theme.colors.navy)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(app.shift?.tellerName ?? "—")
                        .font(.ui(15, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    if !app.branchName.isEmpty {
                        HStack(spacing: Space.xs) {
                            MadarIcon("storefront", size: 11).foregroundStyle(theme.colors.textMuted)
                            Text(app.branchName).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                        }
                    }
                }
                Spacer(minLength: 0)
                if let role = app.session?.role, !role.isEmpty {
                    StatusChip(label: role.replacingOccurrences(of: "_", with: " ").uppercased(), tone: .info)
                }
            }
        }
    }
}

// MARK: - Printer + device code

private struct PrinterCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    // This till's code — the <DEVICE> segment of every order_ref (e.g. T1).
    // `private(set)` on the model: writes route through the core setter, which
    // sanitizes to short A-Z0-9 and ignores blank.
    private var deviceCode: Binding<String> {
        Binding(get: { app.deviceCode }, set: { app.setDeviceCode($0) })
    }
    // Printer host + brand live in the CORE device config; writes route through
    // `setDevicePrinter` (host:port + brand), keeping the other field.
    private var printerHost: Binding<String> {
        Binding(get: { app.printerHost }, set: { app.setDevicePrinter(host: $0, brand: app.printerBrand) })
    }

    var body: some View {
        SettingsCard(t("settings.printer")) {
            MadarTextField(placeholder: t("settings.device_code_hint"), text: deviceCode,
                            icon: "number", caps: .words)
            Text(t("settings.device_code_caption"))
                .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
            MadarTextField(placeholder: t("settings.printer_hint"), text: printerHost, icon: "printer")
            HStack(spacing: Space.sm) {
                SettingsChip(t("settings.printer_epson"), app.printerBrand == .epson) {
                    app.setDevicePrinter(host: app.printerHost, brand: .epson)
                }
                SettingsChip(t("settings.printer_star"), app.printerBrand == .star) {
                    app.setDevicePrinter(host: app.printerHost, brand: .star)
                }
            }
        }
    }
}

// MARK: - Till (drawer) binding

// Which POS drawer this device controls. Hidden on kitchen devices (they bind a
// station, not a till) and when there are none.
private struct TillCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        if !app.isKitchenDevice && !app.tills.isEmpty {
            SettingsCard(t("settings.till")) {
                tillRow(t("settings.till_default"), app.deviceConfig.tillId == nil) { app.setDeviceTill(nil) }
                ForEach(app.tills, id: \.id) { till in
                    tillRow(till.name, app.deviceConfig.tillId == till.id) { app.setDeviceTill(till.id) }
                }
            }
        }
    }

    private func tillRow(_ label: String, _ selected: Bool, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: Space.sm) {
                MadarIcon(selected ? "checkmark.circle" : "circle", size: IconSize.lg)
                    .foregroundStyle(selected ? theme.colors.accent : theme.colors.textMuted)
                Text(label).font(.ui(14, .medium)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.pressable(scale: 0.97))
    }
}

// MARK: - LAN relay

private struct LanCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    // Optional fixed hub-IP for the LAN relay when mDNS auto-discovery can't reach
    // peers. Writes route through the core (`setDeviceLanHub`), which registers it
    // live if the relay is running and clears on blank.
    private var hub: Binding<String> {
        Binding(get: { app.lanHub }, set: { app.setDeviceLanHub($0) })
    }

    var body: some View {
        SettingsCard(t("settings.lan")) {
            MadarTextField(placeholder: t("settings.lan_hub_hint"), text: hub, icon: "wifi")
            Text(t("settings.lan_caption"))
                .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
            SettingsInfoRow(app.lanRelayActive ? t("settings.lan_active") : t("settings.lan_offline"),
                            app.lanRelayActive ? "\(app.lanPeerCount) \(t("settings.lan_peers"))" : "—")
        }
    }
}

// MARK: - Diagnostics

private struct DiagnosticsCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        SettingsCard(t("settings.diagnostics")) {
            SettingsInfoRow(t("settings.version"), app.core.version())
            SettingsInfoRow(t("settings.server"), app.core.baseUrl())
            SettingsInfoRow(t("settings.pending"), "\(app.pendingCount)")
            // Realtime (SSE) channel health — the teller's order alerts ride this;
            // surfacing it makes a silent drop diagnosable.
            SettingsInfoRow(t("settings.realtime"), app.realtimeConnected ? t("settings.realtime_on") : t("settings.realtime_off"))
            if !app.diagnostics.isEmpty {
                Divider().background(theme.colors.borderLight)
                HStack {
                    Text(t("settings.recent_warnings"))
                        .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                    Spacer()
                    Button { app.clearDiagnostics() } label: {
                        Text(t("settings.clear")).font(.ui(12, .semibold)).foregroundStyle(theme.colors.accent)
                    }
                    .buttonStyle(.plain)
                }
                ForEach(Array(app.diagnostics.prefix(15).enumerated()), id: \.offset) { _, e in
                    VStack(alignment: .leading, spacing: 1) {
                        Text(e.message).font(.ui(12))
                            .foregroundStyle(e.level == "error" ? theme.colors.danger : theme.colors.warning)
                            .fixedSize(horizontal: false, vertical: true)
                        Text(e.at).font(.ui(10)).foregroundStyle(theme.colors.textMuted)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .task {
            app.loadDiagnostics()
            if !app.isKitchenDevice { await app.loadTills() }
        }
    }
}

// MARK: - Shared parts

/// A titled settings card — muted uppercase label above a hairline-bordered
/// `surface` panel. Takes a `@ViewBuilder` slot, not primitive content.
private struct SettingsCard<Content: View>: View {
    @Environment(\.theme) private var theme
    let title: String
    @ViewBuilder let content: () -> Content

    init(_ title: String, @ViewBuilder content: @escaping () -> Content) {
        self.title = title
        self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            Text(title.uppercased())
                .font(.ui(12, .bold)).tracking(0.6).foregroundStyle(theme.colors.textMuted)
            VStack(alignment: .leading, spacing: Space.md) { content() }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Space.lg)
                .background(theme.colors.surface)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                        .strokeBorder(theme.colors.borderLight, lineWidth: 1)
                )
                .elevation(.card)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        }
    }
}

/// A pill-toggle chip — accent fill when active, `surfaceAlt` + border otherwise.
private struct SettingsChip: View {
    @Environment(\.theme) private var theme
    let label: String
    let active: Bool
    let action: () -> Void

    init(_ label: String, _ active: Bool, action: @escaping () -> Void) {
        self.label = label
        self.active = active
        self.action = action
    }

    var body: some View {
        Button { Haptics.selection(); action() } label: {
            Text(label)
                .font(.ui(13, .semibold))
                .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, Space.md)
                .background(active ? theme.colors.accent : theme.colors.surfaceAlt)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }
}

private struct SettingsInfoRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String

    init(_ label: String, _ value: String) {
        self.label = label
        self.value = value
    }

    var body: some View {
        HStack {
            Text(label).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Spacer(minLength: Space.md)
            Text(value).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                .lineLimit(1).truncationMode(.middle)
        }
    }
}
