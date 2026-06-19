// Login screen (PLAN §6). Two modes share one core entry point: tellers sign in
// with name + PIN (works online AND offline via the cached org bundle), managers
// with email + password (online only). The view never decides online vs offline
// — `core.sign_in` does.
import SwiftUI

struct LoginView: View {
    @ObservedObject var app: AppModel

    private enum Mode: String, CaseIterable, Identifiable {
        case teller = "Teller", manager = "Manager"
        var id: String { rawValue }
    }

    @State private var mode: Mode = .teller
    @State private var name = ""
    @State private var pin = ""
    @State private var email = ""
    @State private var password = ""
    @State private var editingBranch = false

    var body: some View {
        VStack(spacing: 20) {
            Spacer(minLength: 24)

            Image(systemName: "cup.and.saucer.fill")
                .font(.system(size: 40))
                .foregroundStyle(.tint)
            Text("Sufrix POS").font(.largeTitle.bold())

            Picker("Mode", selection: $mode) {
                ForEach(Mode.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 320)

            Group {
                switch mode {
                case .teller: tellerFields
                case .manager: managerFields
                }
            }
            .frame(maxWidth: 320)
            .textFieldStyle(.roundedBorder)

            if let error = app.errorMessage {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }

            Button(action: submit) {
                if app.isBusy {
                    ProgressView()
                } else {
                    Text("Sign in").frame(maxWidth: 320)
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(app.isBusy || !canSubmit)

            Spacer()
        }
        .padding(32)
        .animation(.default, value: mode)
    }

    // ── mode field groups ────────────────────────────────────────────────────

    @ViewBuilder private var tellerFields: some View {
        VStack(spacing: 12) {
            TextField("Name", text: $name)
                .textContentType(.username)
                #if os(iOS)
                .autocapitalization(.words)
                #endif
            SecureField("PIN", text: $pin)
                #if os(iOS)
                .keyboardType(.numberPad)
                #endif

            if app.branchId.isEmpty || editingBranch {
                TextField("Device branch ID", text: $app.branchId)
                    .font(.footnote)
                Text("Set once when configuring this device.")
                    .font(.caption2).foregroundStyle(.secondary)
            } else {
                Button("Branch: \(shortBranch)") { editingBranch = true }
                    .font(.caption).buttonStyle(.plain).foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder private var managerFields: some View {
        VStack(spacing: 12) {
            TextField("Email", text: $email)
                .textContentType(.emailAddress)
                #if os(iOS)
                .keyboardType(.emailAddress)
                .autocapitalization(.none)
                #endif
            SecureField("Password", text: $password)
                .textContentType(.password)
        }
    }

    // ── derived ──────────────────────────────────────────────────────────────

    private var canSubmit: Bool {
        switch mode {
        case .teller:
            return !name.trimmed.isEmpty && !pin.isEmpty && !app.branchId.trimmed.isEmpty
        case .manager:
            return !email.trimmed.isEmpty && !password.isEmpty
        }
    }

    private var shortBranch: String { String(app.branchId.prefix(8)) }

    private func submit() {
        Task {
            switch mode {
            case .teller:
                await app.signInTeller(name: name.trimmed, pin: pin)
            case .manager:
                await app.signInManager(email: email.trimmed, password: password)
            }
        }
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
