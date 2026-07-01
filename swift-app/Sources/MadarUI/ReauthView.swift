// Re-auth prompt shown when the bearer token expired mid-shift (`syncAuthPaused`).
// The teller who owns the OPEN shift re-enters their PIN to resume syncing — same
// teller, no handover (`login` un-parks the queue and drains the backlog). The
// escape hatch closes the shift and routes to the login screen for a new teller.
// Mirror of the Compose ReauthScreen.
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
            ReauthHeader(
                title: t("chrome.reauth_title"),
                subtitle: t("chrome.reauth_body"),
                onClose: onClose
            )
            // Deliberate rhythm mirrors the Login PIN pad (not a flat stack): the
            // identity pill sits up top, then `xxl` of air above the pad (and `xl`
            // below) so it reads as the hero, `sm` before the CTA, and a clear gap
            // down to the quiet escape hatch.
            VStack(spacing: 0) {
                // Locked to the current teller — no name field, just the shared
                // tinted-teal identity pill (same StatusChip the Login branch pill uses).
                StatusChip(label: "\(t("chrome.reauth_as")) \(tellerName)", icon: "person.crop.circle.badge.clock", tone: .accent)

                Spacer().frame(height: Space.xxl)

                PinPad(pin: pin, maxLength: maxPin, onDigit: digit, onBackspace: backspace)

                if let error = app.errorMessage {
                    Spacer().frame(height: Space.sm)
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                }

                Spacer().frame(height: Space.xl)

                // The sign-in CTA carries the weight — bold teal fill, the brightest
                // thing on the sheet (mirrors the Login pad).
                MadarButton(label: t("login.sign_in"), icon: "arrow.right.circle", loading: app.isBusy, height: 52) { submit() }

                Spacer().frame(height: Space.sm)

                // Escape hatch — close the shift and route a different teller to login.
                Button(t("chrome.reauth_switch")) { app.reauthSwitchTeller() }
                    .buttonStyle(.pressable)
                    .font(.ui(13, .semibold)).foregroundStyle(theme.colors.textMuted)
                    .padding(.vertical, Space.xs)
            }
            .padding(.horizontal, Space.lg)
            .padding(.bottom, Space.xl)
            .padding(.top, Space.xl)
        }
        .background(theme.colors.surfaceAlt)
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
        guard pin.count >= 4 else { Haptics.warning(); return }
        Task {
            await app.reauth(pin: pin)
            if app.errorMessage != nil { pin = ""; Haptics.warning() }
        }
    }
}

// MARK: - Header

/// The sheet header — a leading accent-tinted icon tile (the signature tone-tile
/// pattern), the hero title + supporting body, and a trailing close affordance.
private struct ReauthHeader: View {
    @Environment(\.theme) private var theme
    let title: String
    let subtitle: String
    let onClose: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: Space.md) {
            MadarIcon("lock.circle", size: IconSize.lg)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 44, height: 44)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.ui(20, .bold)).foregroundStyle(theme.colors.textPrimary)
                Text(subtitle).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
            }
            Spacer(minLength: 0)
            CloseButton(action: onClose)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

/// The header's close glyph — a bordered surface-alt squircle (matches the order
/// screen's bar-button idiom).
private struct CloseButton: View {
    @Environment(\.theme) private var theme
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            MadarIcon("xmark", size: 14)
                .foregroundStyle(theme.colors.textMuted)
                .frame(width: 32, height: 32)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable)
    }
}

