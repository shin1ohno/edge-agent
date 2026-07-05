import SwiftUI

/// Connections tab root. Phase 1 placeholder — real list + editor land
/// in Phase 3-4 (see `#ios-tab-connections` and `#ios-detail-connection`).
public struct ConnectionsRootView: View {
    public init() {}

    public var body: some View {
        ContentUnavailableView(
            "No connections yet",
            systemImage: "link",
            description: Text("Tie a Nuimo input to a service action to create a connection.")
        )
    }
}
