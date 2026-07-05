import XCTest
@testable import weave

/// Sanity tests for the app-target glue. Domain-model coverage lives in
/// `Packages/WeaveCore/Tests/WeaveCoreTests/`.
final class WeaveAppRootTabTests: XCTestCase {
    func testRootTabEnumHasFourCases() {
        let cases: [RootTabView.Tab] = [.home, .devices, .connections, .services]
        XCTAssertEqual(Set(cases).count, 4)
    }
}
