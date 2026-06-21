// Open-shift — the gate between sign-in and selling, and the continuation of the
// login moment: login confirms WHO you are, this confirms WHAT'S in the drawer.
// A name-first greeting, one isolated hero count field (auto-focused), and a
// single loud primary. On iPad/desktop it splits into the same BrandPanel as
// login; on iPhone it's one calm centered column.
import SwiftUI

struct OpenShiftView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme

    @State private var openingMinor: Int64 = 0

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= 760
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        BrandPanel().frame(width: geo.size.width * 0.5)
                        formColumn(showLogo: false)
                    }
                } else {
                    formColumn(showLogo: true)
                }
            }
        }
    }

    @ViewBuilder private func formColumn(showLogo: Bool) -> some View {
        ScrollView {
            OpenShiftForm(app: app, openingMinor: $openingMinor, showLogo: showLogo)
                .frame(maxWidth: 400)
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
            if showLogo { SufrixMark(size: 56) }

            // ── Greeting (the teller's name IS the hero) ──────────────────────
            VStack(spacing: Space.xs) {
                Text(t("shift.welcome"))
                    .font(.ui(15, .medium)).foregroundStyle(theme.colors.textSecondary)
                Text(app.session?.displayName ?? t("shift.open_title"))
                    .font(.ui(28, .heavy)).foregroundStyle(theme.colors.textPrimary)
                    .multilineTextAlignment(.center)
                if !app.branchName.isEmpty {
                    StatusChip(label: app.branchName, icon: "building.2", tone: .info)
                        .padding(.top, Space.xs)
                }
            }
            .padding(.top, showLogo ? Space.xl : 0)

            // ── Hero count field (the one thing the teller must do) ───────────
            VStack(spacing: Space.md) {
                Text(t("shift.opening_cash"))
                    .font(.ui(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
                    .frame(maxWidth: .infinity)
                AmountField(amountMinor: $openingMinor, currencyCode: currency, autofocus: true)

                // Carried-over suggestion (previous declared closing).
                if app.suggestedOpeningCashMinor > 0 { carryoverHint }

                // Discrepancy reason — only when the count deviates from carryover.
                if needsReason {
                    SufrixTextField(placeholder: t("shift.opening_reason_label"),
                                    text: $reason, icon: "exclamationmark.bubble")
                }

                Text(needsReason ? t("shift.opening_reason_hint") : t("shift.opening_hint"))
                    .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                    .multilineTextAlignment(.center)
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
            SufrixButton(label: t("shift.open_button"), icon: "lock.open", loading: app.isBusy) {
                submit()
            }
            .padding(.top, app.errorMessage == nil ? Space.xl : Space.md)

            // ── Recessive exit ───────────────────────────────────────────────
            SufrixButton(label: t("shift.switch_teller"), variant: .ghost) { app.signOut() }
                .padding(.top, Space.sm)
        }
        .task {
            app.clearError()
            await app.loadOpenShiftPrefill()
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

    /// The carried-over opening-cash suggestion (previous declared closing).
    private var carryoverHint: some View {
        HStack(spacing: Space.sm) {
            Image(systemName: "clock.arrow.circlepath")
                .font(.system(size: 13)).foregroundStyle(theme.colors.textMuted)
            Text(t("shift.suggested_from_close"))
                .font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
            Spacer(minLength: Space.sm)
            Text(Money.format(app.suggestedOpeningCashMinor, currency))
                .font(.money(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
        .background(theme.colors.surfaceAlt)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1)
        )
    }
}
