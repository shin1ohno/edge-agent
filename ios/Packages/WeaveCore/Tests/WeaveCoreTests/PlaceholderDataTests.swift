import XCTest
@testable import WeaveCore

final class PlaceholderDataTests: XCTestCase {
    func testMappingsHaveAtLeastOneFiring() {
        XCTAssertTrue(PlaceholderData.mappings.contains(where: { $0.firing }))
        XCTAssertNotNil(PlaceholderData.firingMapping)
    }

    func testEdgeAgentsIncludeOneOnlineAndOneOffline() {
        XCTAssertTrue(PlaceholderData.edgeAgents.contains(where: { $0.isConnected }))
        XCTAssertTrue(PlaceholderData.edgeAgents.contains(where: { !$0.isConnected }))
    }

    func testServicesIncludeConnectedAndPending() {
        XCTAssertTrue(PlaceholderData.services.contains(where: { $0.isConnected }))
        XCTAssertTrue(PlaceholderData.services.contains(where: { !$0.isConnected }))
        let sonos = PlaceholderData.services.first(where: { $0.id == "sonos" })
        XCTAssertEqual(sonos?.isNew, true)
    }

    func testRecentActivityRowsHaveAllFields() {
        XCTAssertEqual(PlaceholderData.recentActivity.count, 4)
        for entry in PlaceholderData.recentActivity {
            XCTAssertFalse(entry.title.isEmpty)
            XCTAssertFalse(entry.subtitle.isEmpty)
            XCTAssertFalse(entry.when.isEmpty)
        }
    }

    func testMappingDisplayLabelFormat() {
        let m = Mapping(
            deviceId: UUID(),
            serviceType: "roon",
            serviceTarget: "Living",
            routes: []
        )
        XCTAssertEqual(m.serviceDisplayLabel, "Roon · Living")
    }

    func testMappingCodableRoundTrip() throws {
        let original = PlaceholderData.mappings[0]
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Mapping.self, from: data)
        XCTAssertEqual(original, decoded)
    }

    func testDevicesIsAliasForOnboardingSnapshot() {
        XCTAssertEqual(PlaceholderData.devices.count, OnboardingSnapshot.placeholder.foundDevices.count)
        XCTAssertEqual(PlaceholderData.devices.first?.id, OnboardingSnapshot.placeholder.foundDevices.first?.id)
    }

    func testAccentHexFormat() {
        for service in PlaceholderData.services {
            XCTAssertEqual(service.accentHex.count, 6, "accentHex should be 6 hex chars: \(service.id)")
        }
        for entry in PlaceholderData.recentActivity {
            XCTAssertEqual(entry.accentHex.count, 6, "accentHex should be 6 hex chars: \(entry.title)")
        }
    }
}
