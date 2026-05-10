import SwiftUI

/// Devices tab root. Phase 1 placeholder. Real list + detail land in
/// Phase 3-4 — see `#ios-tab-devices` and `#ios-detail-device`.
public struct DevicesRootView: View {
    public init() {}

    public var body: some View {
        ContentUnavailableView(
            "No devices yet",
            systemImage: "dot.radiowaves.left.and.right",
            description: Text("Pair a Nuimo to get started.")
        )
    }
}
