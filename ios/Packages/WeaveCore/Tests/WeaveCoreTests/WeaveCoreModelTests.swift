import XCTest
@testable import WeaveCore

final class WeaveCoreModelTests: XCTestCase {
    func testDeviceCodableRoundTrip() throws {
        let original = Device(
            name: "Nuimo",
            nickname: "sofa",
            room: "living",
            battery: 73,
            firmware: "2.3.0",
            isOnline: true
        )
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Device.self, from: data)
        XCTAssertEqual(original, decoded)
    }

    func testRouteCodableWithVolumeAction() throws {
        let device = Device()
        let service = Service(kind: .roon, displayName: "Roon Living")
        let route = Route(
            input: .rotate,
            device: device.id,
            service: service.id,
            action: .volume(delta: 3),
            smoothness: 0.7
        )
        let data = try JSONEncoder().encode(route)
        let decoded = try JSONDecoder().decode(Route.self, from: data)
        XCTAssertEqual(route, decoded)
        if case let .volume(delta) = decoded.action {
            XCTAssertEqual(delta, 3)
        } else {
            XCTFail("expected .volume action")
        }
    }

    func testServiceStatusErrorRoundTrip() throws {
        let service = Service(
            kind: .hue,
            displayName: "Hue Bridge",
            status: .error("bridge unreachable")
        )
        let data = try JSONEncoder().encode(service)
        let decoded = try JSONDecoder().decode(Service.self, from: data)
        XCTAssertEqual(service, decoded)
    }

    func testInputTypeRawValuesStable() {
        XCTAssertEqual(InputType.rotate.rawValue, "rotate")
        XCTAssertEqual(InputType.longPress.rawValue, "longPress")
        XCTAssertEqual(InputType.swipeLeft.rawValue, "swipeLeft")
    }
}
