// Phase-1 placeholder screen. Proves the core is reachable from SwiftUI and
// shows what the host reads from it. Phase 6 replaces this with the real
// iPhone/iPad layouts (Login → Shift → Catalog/Order → Cart → Payment → Receipt;
// see PLAN.md §6).
import SwiftUI

struct ContentView: View {
    let core: SufrixCore

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "cup.and.saucer.fill")
                .font(.system(size: 44))
                .foregroundStyle(.tint)
            Text("Sufrix POS")
                .font(.largeTitle.bold())
            Text(greet(name: "Teller"))
                .font(.callout)
                .foregroundStyle(.secondary)
            Divider().padding(.vertical, 8)
            row("core version", core.version())
            row("ffi surface", String(ffiSurfaceVersion()))
            row("environment", core.environment())
            row("base URL", core.baseUrl())
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
