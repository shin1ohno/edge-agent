import SwiftUI

/// Services tab root. Phase 1 placeholder — Roon / Hue / HomeKit / Sonos
/// integration cells land in Phase 3 (see `#ios-tab-services`).
public struct ServicesRootView: View {
    public init() {}

    public var body: some View {
        ContentUnavailableView(
            "No services connected",
            systemImage: "square.stack.3d.up",
            description: Text("Connect Roon, Hue, HomeKit, or Sonos to start firing actions.")
        )
    }
}
