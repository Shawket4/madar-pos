// Re-auth prompt shown when the bearer token expired mid-shift (`syncAuthPaused`).
// The teller who owns the OPEN shift re-enters their PIN to resume syncing — same
// teller, no handover (`login` un-parks the queue and drains the backlog). The
// escape hatch closes the shift and routes to the login screen for a new teller.
import SwiftUI

struct ReauthView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var pin = ""
    private let maxPin = 6

    private var tellerName: String { app.session?.displayName ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            header
            VStack(spacing: Space.lg) {
                signedInChip
                PinPad(pin: pin, maxLength: maxPin, onDigit: digit, onBackspace: backspace)
                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                }
                MadarButton(label: t("login.sign_in"), loading: app.isBusy, height: 52) { submit() }
                Button(t("chrome.reauth_switch")) { app.reauthSwitchTeller() }
                    .buttonStyle(.plain)
                    .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
            }
            .padding(.horizontal, Space.lg)
            .padding(.bottom, Space.lg)
            .padding(.top, Space.xs)
        }
        .background(theme.colors.surfaceAlt)
    }

    private var header: some View {
        HStack(alignment: .top, spacing: Space.md) {
            VStack(alignment: .leading, spacing: 3) {
                Text(t("chrome.reauth_title")).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Text(t("chrome.reauth_body")).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
            }
            Spacer(minLength: 0)
            Button { onClose() } label: {
                MadarIcon("xmark", size: 14)
                    .foregroundStyle(theme.colors.textMuted)
                    .frame(width: 32, height: 32)
                    .background(theme.colors.surfaceAlt)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(theme.colors.border, lineWidth: 1))
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private var signedInChip: some View {
        HStack(spacing: 6) {
            MadarIcon("person.crop.circle.badge.clock", size: 13)
            Text("\(t("chrome.reauth_as")) \(tellerName)").font(.ui(13, .semibold))
        }
        .foregroundStyle(theme.colors.accent)
        .padding(.horizontal, 12).padding(.vertical, 7)
        .background(theme.colors.accentBg)
        .clipShape(Capsule())
    }

    private func digit(_ d: String) {
        guard !app.isBusy, pin.count < maxPin else { return }
        app.errorMessage = nil
        pin += d
        if pin.count == maxPin { submit() }
    }
    private func backspace() { guard !pin.isEmpty else { return }; pin.removeLast() }

    private func submit() {
        guard pin.count >= 4 else { Haptics.warning(); return }
        Task {
            await app.reauth(pin: pin)
            if app.errorMessage != nil { pin = ""; Haptics.warning() }
        }
    }
}
