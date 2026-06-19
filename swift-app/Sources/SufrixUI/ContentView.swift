// Signed-in placeholder home. Proves the full auth round-trip from SwiftUI:
// it reads the cached session the core handed back and offers sign-out. Phase 6
// replaces this with the real Shift → Catalog/Order → Cart → Payment → Receipt
// layouts (PLAN §6).
import SwiftUI

struct ContentView: View {
    @ObservedObject var app: AppModel

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "checkmark.seal.fill")
                .font(.system(size: 40))
                .foregroundStyle(.tint)
            Text("Signed in").font(.largeTitle.bold())

            if let s = app.session {
                row("teller", s.displayName)
                row("role", s.role)
                row("session", s.online ? "online" : "offline")
                row("currency", s.currencyCode)
            }
            Divider().padding(.vertical, 8)
            row("core version", app.core.version())
            row("environment", app.core.environment())

            Button("Sign out", role: .destructive) { app.signOut() }
                .buttonStyle(.bordered)
                .padding(.top, 8)
        }
        .padding(32)
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).foregroundStyle(.secondary)
            Spacer()
            Text(value).monospaced()
        }
        .font(.footnote)
    }
}
