// Login — a full-screen brand moment outside the nav shell. Branch-gated
// (replicates Flutter): a till that isn't bound to a branch shows the manager
// device-setup; once configured it shows the teller PIN pad with a reconfigure
// link. Wide screens (iPad / desktop) split into a brand panel + form. All
// strings come from the core's shared i18n table via `@Environment(\.localize)`.
import SwiftUI

struct LoginView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= Responsive.wide
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        // Flutter splits the wide layout 55/45 (brand panel : form).
                        BrandPanel().frame(width: geo.size.width * 0.55)
                        form(showLogo: false)
                    }
                } else {
                    form(showLogo: true)
                }
            }
        }
    }

    @ViewBuilder private func form(showLogo: Bool) -> some View {
        ScrollView {
            Group {
                if app.isBranchConfigured && !app.reconfiguring {
                    TellerForm(app: app, showLogo: showLogo)
                } else {
                    DeviceSetupForm(app: app, showLogo: showLogo)
                }
            }
            .frame(maxWidth: 400)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Space.xxl)
            .padding(.vertical, 48)
        }
    }
}

// MARK: - Teller (configured till)

private struct TellerForm: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    var showLogo: Bool

    @State private var name = ""
    @State private var pin = ""
    @State private var shake: CGFloat = 0

    private let maxPin = 6

    var body: some View {
        // Spacing mirrors Flutter `_buildForm`'s deliberate rhythm (not a flat
        // stack): xs between title/subtitle, sm before the branch chip, xxl after
        // the header block, xl around the PIN pad, sm between button and hint.
        VStack(spacing: 0) {
            if showLogo {
                MadarMark(size: 60)
                Spacer().frame(height: Space.xxl)
            }

            VStack(spacing: Space.xs) {
                Text(t("login.welcome_back")).font(.ui(24, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Text(t("login.subtitle")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
            }

            VStack(spacing: Space.xs) {
                StatusChip(label: branchLabel, icon: "building.2", tone: .info)
                Button(t("login.reconfigure")) { app.beginReconfigure() }
                    .buttonStyle(.plain)
                    .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
            }
            .padding(.top, Space.sm)

            Spacer().frame(height: Space.xxl)

            MadarTextField(placeholder: t("login.name"), text: $name, icon: "person", disabled: app.isBusy, caps: .words)

            Spacer().frame(height: Space.xl)

            PinPad(pin: pin, maxLength: maxPin, onDigit: digit, onBackspace: backspace)

            if let error = app.errorMessage {
                NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    .padding(.top, Space.sm)
            }

            Spacer().frame(height: Space.xl)

            MadarButton(label: t("login.sign_in"), loading: app.isBusy, height: 52) { submit() }

            Spacer().frame(height: Space.sm)

            Text(t("login.pin_hint"))
                .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                .multilineTextAlignment(.center)
        }
        .modifier(Shake(animatableData: shake))
    }

    private var branchLabel: String {
        app.branchName.isEmpty ? "\(t("login.branch")) \(app.branchId.prefix(8))" : app.branchName
    }

    private func digit(_ d: String) {
        guard !app.isBusy, pin.count < maxPin else { return }
        app.errorMessage = nil
        pin += d
        if pin.count == maxPin { submit() }
    }

    private func backspace() {
        guard !pin.isEmpty else { return }
        pin.removeLast()
    }

    private func submit() {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, pin.count >= 4 else { failShake(); return }
        Task {
            await app.signInTeller(name: trimmed, pin: pin)
            if app.errorMessage != nil {
                pin = ""
                failShake()
            }
        }
    }

    private func failShake() {
        Haptics.impact()
        withAnimation(.linear(duration: 0.4)) { shake += 1 }
    }
}

// MARK: - Device setup (manager binds the till to a branch)

private struct DeviceSetupForm: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    var showLogo: Bool

    @State private var email = ""
    @State private var password = ""

    private var picking: Bool { app.setupPhase == .pickBranch }

    var body: some View {
        VStack(spacing: Space.lg) {
            if showLogo { MadarMark(size: 56) }

            VStack(spacing: Space.xs) {
                Text(picking ? t("setup.choose_branch") : t("setup.title"))
                    .font(.ui(22, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Text(picking ? t("setup.choose_branch_desc") : t("setup.desc"))
                    .font(.ui(13.5)).foregroundStyle(theme.colors.textSecondary)
                    .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
            }
            .padding(.bottom, Space.sm)

            if picking {
                ForEach(app.branches, id: \.id) { branch in branchRow(branch) }
            } else {
                MadarTextField(placeholder: t("setup.email"), text: $email, icon: "envelope",
                                disabled: app.isBusy, keyboard: .email)
                MadarTextField(placeholder: t("setup.password"), text: $password, icon: "lock",
                                secure: true, disabled: app.isBusy)
            }

            if let error = app.errorMessage {
                NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
            }

            if !picking {
                MadarButton(label: t("setup.continue"), loading: app.isBusy) {
                    Task {
                        await app.authenticateManager(
                            email: email.trimmingCharacters(in: .whitespaces),
                            password: password)
                    }
                }
            }
            if picking || app.isBranchConfigured {
                MadarButton(label: t("setup.cancel"), variant: .ghost) { app.cancelReconfigure() }
            }
        }
    }

    private func branchRow(_ branch: BranchView) -> some View {
        Button {
            Haptics.selection()
            app.bindBranch(branch)
        } label: {
            HStack(spacing: Space.md) {
                MadarIcon("building.2", size: IconSize.sm).foregroundStyle(theme.colors.textMuted)
                Text(branch.name).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                MadarIcon("chevron.forward", size: 13).foregroundStyle(theme.colors.textMuted)
            }
            .padding(.horizontal, 14).padding(.vertical, 14)
            .frame(maxWidth: .infinity)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.borderLight, lineWidth: 1)
            )
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable)
    }
}

// MARK: - Shake (error feedback)

private struct Shake: GeometryEffect {
    var amount: CGFloat = 8
    var shakesPerUnit: CGFloat = 3
    var animatableData: CGFloat

    func effectValue(size: CGSize) -> ProjectionTransform {
        let dx = amount * sin(animatableData * .pi * shakesPerUnit * 2)
        return ProjectionTransform(CGAffineTransform(translationX: dx, y: 0))
    }
}
