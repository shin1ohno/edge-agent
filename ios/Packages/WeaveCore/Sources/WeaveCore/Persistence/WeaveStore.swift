import Foundation
import SwiftData

/// Owns the SwiftData `ModelContainer` for weave's local persistence.
///
/// Phase 1 exposes only the container factory and a `preview` in-memory
/// flavour used by SwiftUI previews and unit tests. CRUD methods land in
/// Phase 3 once the tab roots need to read/write live data.
public actor WeaveStore {
    public let container: ModelContainer

    public init(container: ModelContainer) {
        self.container = container
    }

    public static func makeContainer(inMemory: Bool = false) throws -> ModelContainer {
        let schema = Schema([
            StoredDevice.self,
            StoredService.self,
            StoredRoute.self,
        ])
        let config = ModelConfiguration(
            schema: schema,
            isStoredInMemoryOnly: inMemory,
            cloudKitDatabase: .none
        )
        return try ModelContainer(for: schema, configurations: [config])
    }

    /// Convenience for SwiftUI `#Preview` blocks: an in-memory container
    /// that survives only for the lifetime of the preview process.
    public static func preview() throws -> WeaveStore {
        WeaveStore(container: try makeContainer(inMemory: true))
    }
}
