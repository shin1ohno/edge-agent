import XCTest
@testable import WeaveCore

final class DefaultsKeysTests: XCTestCase {
    func testDefaultsKeysAreUnique() {
        let keys: [String] = [
            DefaultsKeys.onboardingCompleted,
            DefaultsKeys.useServer,
            DefaultsKeys.serverURL,
            DefaultsKeys.batteryAlertThreshold,
            DefaultsKeys.openAtLaunch,
        ]
        XCTAssertEqual(Set(keys).count, keys.count, "DefaultsKeys must not collide")
    }

    func testDefaultsKeysShareWeavePrefix() {
        let keys = [
            DefaultsKeys.onboardingCompleted,
            DefaultsKeys.useServer,
            DefaultsKeys.serverURL,
            DefaultsKeys.batteryAlertThreshold,
            DefaultsKeys.openAtLaunch,
        ]
        for key in keys {
            XCTAssertTrue(key.hasPrefix("weave."), "\(key) should be namespaced under weave.")
        }
    }

    func testOnboardingSnapshotPlaceholderShape() {
        let snapshot = OnboardingSnapshot.placeholder
        XCTAssertEqual(snapshot.foundDevices.count, 3)
        XCTAssertEqual(snapshot.discoveredServers.count, 2)
        XCTAssertEqual(snapshot.foundDevices.first?.nickname, "sofa")
        XCTAssertTrue(snapshot.foundDevices.first?.isOnline ?? false)
        XCTAssertEqual(snapshot.discoveredServers.first?.hostname, "weave-server.local")
    }

    func testOnboardingSnapshotCodableRoundTrip() throws {
        let snapshot = OnboardingSnapshot.placeholder
        let data = try JSONEncoder().encode(snapshot)
        let decoded = try JSONDecoder().decode(OnboardingSnapshot.self, from: data)
        XCTAssertEqual(snapshot, decoded)
    }
}
