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
                header
                ScrollView {
                    VStack(spacing: Space.lg) {
                        if let error = app.errorMessage {
                            NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning)
                        }
                        accountCard
                        appearanceCard
                        languageCard
                        printerCard
                        deviceCard
                        diagnosticsCard
                        SufrixButton(label: t("settings.sign_out"),
                                     icon: "rectangle.portrait.and.arrow.right", variant: .danger) {
                            // Sign-out (→ login) requires a closed drawer first.
                            if app.hasOpenShift {
                                app.flagError(t("settings.sign_out_shift_open"))
                            } else {
                                app.signOut()
                                onClose()
                            }
                        }
                    }
                    .frame(maxWidth: 480)
                    .frame(maxWidth: .infinity)
                    .padding(Space.lg)
                }
            }
        }
        .onAppear { app.clearError() }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                Image(systemName: "chevron.backward").font(.system(size: 17, weight: .semibold))
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

    private var accountCard: some View {
        card(t("settings.account")) {
            HStack(spacing: Space.md) {
                ZStack {
                    Circle().fill(theme.colors.accentBg).frame(width: 44, height: 44)
                    Text(String((app.shift?.tellerName ?? "?").prefix(1)).uppercased())
                        .font(.ui(16, .heavy)).foregroundStyle(theme.colors.accent)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(app.shift?.tellerName ?? "—")
                        .font(.ui(15, .bold)).foregroundStyle(theme.colors.textPrimary)
                    if !app.branchName.isEmpty {
                        Text(app.branchName).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                    }
                }
                Spacer(minLength: 0)
                if let role = app.session?.role, !role.isEmpty {
                    StatusChip(label: role.capitalized, tone: .accent)
                }
            }
        }
    }

    private var appearanceCard: some View {
        card(t("settings.appearance")) {
            HStack(spacing: Space.sm) {
                chip(t("settings.theme_light"), app.themeMode == .light) { app.themeMode = .light }
                chip(t("settings.theme_dark"), app.themeMode == .dark) { app.themeMode = .dark }
                chip(t("settings.theme_system"), app.themeMode == .system) { app.themeMode = .system }
            }
        }
    }

    private var languageCard: some View {
        card(t("settings.language")) {
            HStack(spacing: Space.sm) {
                chip("English", app.locale.hasPrefix("en")) { app.locale = "en" }
                chip("العربية", app.locale.hasPrefix("ar")) { app.locale = "ar" }
            }
        }
    }

    private var deviceCard: some View {
        card(t("settings.device")) {
            Button {
                Haptics.selection()
                // Reconfiguring re-provisions the device — only with a closed drawer.
                if app.hasOpenShift {
                    app.flagError(t("settings.reconfigure_shift_open"))
                } else {
                    app.beginReconfigure()
                    onClose()
                }
            } label: {
                HStack {
                    Image(systemName: "building.2").foregroundStyle(theme.colors.textMuted)
                    Text(t("settings.reconfigure")).font(.ui(14)).foregroundStyle(theme.colors.textPrimary)
                    Spacer()
                    Image(systemName: "chevron.forward").font(.system(size: 13)).foregroundStyle(theme.colors.textMuted)
                }
            }
            .buttonStyle(.pressable)
        }
    }

    private var printerCard: some View {
        card(t("settings.printer")) {
            SufrixTextField(placeholder: t("settings.printer_hint"), text: $app.printerHost, icon: "printer")
            HStack(spacing: Space.sm) {
                chip(t("settings.printer_epson"), app.printerBrand == .epson) { app.printerBrand = .epson }
                chip(t("settings.printer_star"), app.printerBrand == .star) { app.printerBrand = .star }
            }
        }
    }

    private var diagnosticsCard: some View {
        card(t("settings.diagnostics")) {
            infoRow(t("settings.version"), app.core.version())
            infoRow(t("settings.server"), app.core.baseUrl())
            infoRow(t("settings.pending"), "\(app.pendingCount)")
            if !app.diagnostics.isEmpty {
                Divider().background(theme.colors.border)
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
        .task { app.loadDiagnostics() }
    }

    // MARK: - Parts

    private func card<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            Text(title.uppercased())
                .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
            VStack(alignment: .leading, spacing: Space.md) { content() }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Space.lg)
                .background(theme.colors.surface)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                        .strokeBorder(theme.colors.border, lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        }
    }

    private func chip(_ label: String, _ active: Bool, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            Text(label)
                .font(.ui(13, .semibold))
                .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
                .background(active ? theme.colors.accent : theme.colors.surfaceAlt)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }

    private func infoRow(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Spacer(minLength: Space.md)
            Text(value).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                .lineLimit(1).truncationMode(.middle)
        }
    }
}
