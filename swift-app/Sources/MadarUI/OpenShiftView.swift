// Open-shift — the gate between sign-in and selling, and the continuation of the
// login moment: login confirms WHO you are, this confirms WHAT'S in the drawer.
// A name-first greeting, one isolated hero count field (auto-focused), and a
// single loud primary. On iPad/desktop it splits into the same BrandPanel as
// login; on iPhone it's one calm centered column.
import SwiftUI

struct OpenShiftView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var openingMinor: Int64 = 0

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= Responsive.wide
            ZStack(alignment: .top) {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        BrandPanel().frame(width: geo.size.width * 0.5)
                        formColumn(showLogo: false)
                    }
                } else {
                    formColumn(showLogo: true)
                }
                // Top-pinned chrome so a teller WAITING on the open-shift screen
                // still sees + recovers connectivity / a genuine session expiry —
                // not only on the order screen. The auth-paused banner only appears
                // when the cached JWT has actually expired (the core gates it now).
                VStack(spacing: Space.sm) {
                    if !app.isOnline {
                        NoticeBanner(icon: "wifi.slash", text: t("chrome.offline_banner"), tone: .warning)
                    }
                    if app.syncAuthPaused {
                        Button { app.clearError(); app.showReauth = true } label: {
                            NoticeBanner(icon: "lock.circle", text: t("chrome.auth_paused"),
                                         tone: .danger, actionLabel: t("chrome.auth_paused_action"))
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, Space.lg)
                .padding(.top, Space.sm)
            }
            // Re-auth the SAME teller to resume sync (mirrors the order screen). The
            // two hosts never coexist — Order and OpenShift are exclusive routes — so
            // there's no double-presentation.
            .madarSheet(isPresented: $app.showReauth, size: .hug, maxWidth: 440) { dismiss in
                ReauthView(app: app, onClose: dismiss)
            }
        }
    }

    @ViewBuilder private func formColumn(showLogo: Bool) -> some View {
        ScrollView {
            OpenShiftForm(app: app, openingMinor: $openingMinor, showLogo: showLogo)
                .frame(maxWidth: 480)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Space.xxl)
                .padding(.vertical, 48)
        }
        #if os(iOS)
        .scrollDismissesKeyboard(.interactively)
        #endif
    }
}

private struct OpenShiftForm: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Binding var openingMinor: Int64
    var showLogo: Bool
    @State private var reason = ""

    private var currency: String { app.session?.currencyCode ?? "" }
    /// The count deviates from the carried-over closing → a reason is required.
    private var needsReason: Bool {
        app.suggestedOpeningCashMinor > 0 && openingMinor != app.suggestedOpeningCashMinor
    }

    var body: some View {
        // spacing 0 + explicit per-gap padding → deliberate hierarchy (the hero
        // count field gets the isolating Space.xxl gap above it).
        VStack(spacing: 0) {
            if showLogo { MadarMark(size: 56) }

            // ── Greeting (the teller's name IS the hero) ──────────────────────
            VStack(spacing: Space.xs) {
                Text(t("shift.welcome"))
                    .font(.ui(15, .medium)).foregroundStyle(theme.colors.textSecondary)
                Text(app.session?.displayName ?? t("shift.open_title"))
                    .font(.ui(28, .black)).tracking(-0.5).foregroundStyle(theme.colors.textPrimary)
                    .multilineTextAlignment(.center)
                if !app.branchName.isEmpty {
                    StatusChip(label: app.branchName, icon: "building.2", tone: .info)
                        .padding(.top, Space.xs)
                }
            }
            .padding(.top, showLogo ? Space.xl : 0)

            // ── Hero count field (the one thing the teller must do) ───────────
            // Wrapped in the shared bordered surface card — matches the Order
            // screen's raised, hairline-bordered surfaces. Section-labelled, the
            // hero figure sits on its own elevated panel rather than floating on
            // the page background.
            MadarCard(spacing: Space.md) {
                SectionHeader(text: t("shift.opening_cash"), icon: "banknote")
                AmountField(amountMinor: $openingMinor, currencyCode: currency, autofocus: true)

                // Carried-over suggestion (previous declared closing).
                if app.suggestedOpeningCashMinor > 0 {
                    CarryoverHint(suggestedMinor: app.suggestedOpeningCashMinor, currency: currency)
                }

                // Discrepancy reason — only when the count deviates from carryover.
                if needsReason {
                    MadarTextField(placeholder: t("shift.opening_reason_label"),
                                    text: $reason, icon: "exclamationmark.bubble")
                }

                Text(needsReason ? t("shift.opening_reason_hint") : t("shift.opening_hint"))
                    .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, Space.xxl)
            .animation(Motion.standard, value: needsReason)

            // ── Error (next to the action that triggers it) ───────────────────
            if let error = app.errorMessage {
                NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    .padding(.top, Space.xl)
            }

            // ── Primary action ───────────────────────────────────────────────
            MadarButton(label: t("shift.open_button"), icon: "lock.open", loading: app.isBusy) {
                submit()
            }
            .padding(.top, app.errorMessage == nil ? Space.xl : Space.md)

            // ── Recessive exit ───────────────────────────────────────────────
            MadarButton(label: t("shift.switch_teller"), variant: .ghost) { app.signOut() }
                .padding(.top, Space.sm)
        }
        .task {
            app.clearError()
            // Before prompting to open a NEW shift, check whether an ACTIVE shift
            // already exists (local cache or server) and adopt it — routing then
            // flips straight to Order. Stops a mid-shift teller being asked to
            // open a duplicate.
            await app.reconcileShift()
            await app.loadOpenShiftPrefill()
        }
        // Connectivity heartbeat here too: a teller who landed on open-shift while
        // offline re-adopts their active shift the moment the network returns
        // (reconcile-on-reconnect lives in refreshConnectivity).
        .task {
            while !Task.isCancelled {
                await app.refreshConnectivity()
                try? await Task.sleep(nanoseconds: 15_000_000_000)
            }
        }
        .onChange(of: app.suggestedOpeningCashMinor) { suggested in
            // Prefill the count once, only while still untouched.
            if openingMinor == 0 && suggested > 0 { openingMinor = suggested }
        }
    }

    /// Validate the discrepancy reason, then open the shift.
    private func submit() {
        if needsReason && reason.trimmingCharacters(in: .whitespaces).isEmpty {
            app.flagError(t("shift.opening_reason_required"))
            return
        }
        let editReason = needsReason ? reason : nil
        Task { await app.openShift(openingCashMinor: openingMinor, editReason: editReason) }
    }
}

/// The carried-over opening-cash suggestion (previous declared closing) — a tinted
/// teal block carrying the prior figure as bold teal money, the twin of
/// CloseShift's ExpectedCashBlock (the figure this open count reconciles against).
/// Its own `View` struct so it owns its environment and recomputes independently.
private struct CarryoverHint: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let suggestedMinor: Int64
    let currency: String

    var body: some View {
        HStack(spacing: Space.sm) {
            MadarIcon("clock.arrow.circlepath", size: IconSize.sm).foregroundStyle(theme.colors.accent)
            Text(t("shift.suggested_from_close"))
                .font(.ui(12, .bold)).foregroundStyle(theme.colors.accent)
            Spacer(minLength: Space.sm)
            Text(Money.format(suggestedMinor, currency))
                .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
        }
        .padding(14)
        .background(theme.colors.accentBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}
