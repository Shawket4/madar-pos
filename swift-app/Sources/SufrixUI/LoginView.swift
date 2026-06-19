// Login — a full-screen brand moment outside the nav shell. Branch-gated
// (replicates Flutter): a till that isn't bound to a branch shows the manager
// device-setup; once configured it shows the teller PIN pad with a reconfigure
// link. Wide screens (iPad / desktop) split into a brand panel + form.
import SwiftUI

struct LoginView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= 760
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        BrandPanel().frame(width: geo.size.width * 0.5)
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
            VStack(spacing: 0) {
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
}

// MARK: - Teller (configured till)

private struct TellerForm: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    var showLogo: Bool

    @State private var name = ""
    @State private var pin = ""
    @State private var shake: CGFloat = 0

    private let maxPin = 6

    var body: some View {
        VStack(spacing: Space.xl) {
            if showLogo { SufrixMark(size: 60) }

            VStack(spacing: Space.xs) {
                Text("Welcome back").font(.ui(24, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Text("Sign in to open your till")
                    .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
            }

            VStack(spacing: Space.xs) {
                StatusChip(label: branchLabel, icon: "building.2", tone: .info)
                Button("Reconfigure device") { app.beginReconfigure() }
                    .buttonStyle(.plain)
                    .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
            }

            SufrixTextField(placeholder: "Name", text: $name, icon: "person", disabled: app.isBusy, caps: .words)

            PinPad(pin: pin, maxLength: maxPin, onDigit: digit, onBackspace: backspace)

            if let error = app.errorMessage {
                NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
            }

            SufrixButton(label: "Sign in", loading: app.isBusy) { submit() }
            Text("PIN auto-submits at 6 digits")
                .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
        }
        .modifier(Shake(animatableData: shake))
    }

    private var branchLabel: String {
        app.branchName.isEmpty ? "Branch \(app.branchId.prefix(8))" : app.branchName
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
    var showLogo: Bool

    @State private var email = ""
    @State private var password = ""

    private var picking: Bool { app.setupPhase == .pickBranch }

    var body: some View {
        VStack(spacing: Space.lg) {
            if showLogo { SufrixMark(size: 56) }

            VStack(spacing: Space.xs) {
                Text(picking ? "Choose a branch" : "Configure this till")
                    .font(.ui(22, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Text(picking
                     ? "Bind this till to one of your branches."
                     : "A manager signs in to bind this device to a branch. Tellers sign in after.")
                    .font(.ui(13.5)).foregroundStyle(theme.colors.textSecondary)
                    .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
            }
            .padding(.bottom, Space.sm)

            if picking {
                ForEach(app.branches, id: \.id) { branch in branchRow(branch) }
            } else {
                SufrixTextField(placeholder: "Manager email", text: $email, icon: "envelope",
                                disabled: app.isBusy, keyboard: .email)
                SufrixTextField(placeholder: "Password", text: $password, icon: "lock",
                                secure: true, disabled: app.isBusy)
            }

            if let error = app.errorMessage {
                NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
            }

            if !picking {
                SufrixButton(label: "Continue", loading: app.isBusy) {
                    Task {
                        await app.authenticateManager(
                            email: email.trimmingCharacters(in: .whitespaces),
                            password: password)
                    }
                }
            }
            if picking || app.isBranchConfigured {
                SufrixButton(label: "Cancel", variant: .ghost) { app.cancelReconfigure() }
            }
        }
    }

    private func branchRow(_ branch: BranchView) -> some View {
        Button {
            Haptics.selection()
            app.bindBranch(branch)
        } label: {
            HStack(spacing: Space.md) {
                Image(systemName: "building.2").foregroundStyle(theme.colors.textMuted)
                Text(branch.name).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                Image(systemName: "chevron.right").font(.system(size: 13)).foregroundStyle(theme.colors.textMuted)
            }
            .padding(.horizontal, 14).padding(.vertical, 14)
            .frame(maxWidth: .infinity)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable)
    }
}

// MARK: - Brand panel (wide layout)

private struct BrandPanel: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ZStack {
            theme.colors.surfaceAlt.ignoresSafeArea()
            // Faded watermark mark.
            SufrixMark(size: 360, armColor: theme.colors.accent.opacity(0.06), dotColor: theme.colors.accent.opacity(0.06))
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                .offset(x: 80, y: 80)
                .clipped()

            VStack(alignment: .leading, spacing: 0) {
                SufrixLockup(markSize: 30, textSize: 24)
                Spacer()
                Text("Welcome\nback.")
                    .font(.ui(44, .black))
                    .foregroundStyle(theme.colors.textPrimary)
                    .lineSpacing(2)
                Text("Sign in to open your till. Works online and off — your sales keep flowing either way.")
                    .font(.ui(15)).foregroundStyle(theme.colors.textSecondary)
                    .lineSpacing(4)
                    .frame(maxWidth: 300, alignment: .leading)
                    .padding(.top, Space.lg)
                Spacer()
                HStack(spacing: Space.sm) {
                    Circle().fill(theme.colors.accent).frame(width: 6, height: 6)
                    Text("© 2026 Sufrix").font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .padding(48)
        }
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
