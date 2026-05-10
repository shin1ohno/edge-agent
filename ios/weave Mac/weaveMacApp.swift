import SwiftUI

/// Mac target is an intentional empty shell for Phase 1 — Universal
/// binary covers Mac as a build target only. macOS UI work is deferred
/// per `handoff/CLAUDE_CODE_HANDOFF.md` scope.
@main
struct WeaveMacApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(spacing: 8) {
                Text("weave for Mac")
                    .font(.title)
                Text("coming soon")
                    .foregroundStyle(.secondary)
            }
            .padding(48)
            .frame(minWidth: 480, minHeight: 320)
        }
    }
}
