import Foundation
import WeaveCore

/// Surface area a Nuimo BLE backend exposes to the rest of the app.
/// Phase 1 ships only the protocol shape and a mock implementation; the
/// real `CBCentralManager`-backed driver moves over from `WeaveIos/Core/`
/// in Phase 5.
public protocol NuimoDriver: AnyObject, Sendable {
    /// Currently paired / known devices, observable as a stream.
    var devices: AsyncStream<[Device]> { get }

    /// Begin scanning for nearby unpaired Nuimo peripherals.
    func startScan() async

    /// Stop any in-flight scan.
    func stopScan() async

    /// Forget a previously paired device.
    func forget(deviceId: Device.ID) async
}

/// Stub used during Phase 1 so dependent targets compile without
/// CoreBluetooth wiring. Emits an empty device list on subscribe.
public final class MockNuimoDriver: NuimoDriver, @unchecked Sendable {
    public init() {}

    public var devices: AsyncStream<[Device]> {
        AsyncStream { continuation in
            continuation.yield([])
            continuation.finish()
        }
    }

    public func startScan() async {
        // TODO(Phase 5): port WeaveIos/Core/BleBridge.swift scan logic.
    }

    public func stopScan() async {
        // TODO(Phase 5): port WeaveIos/Core/BleBridge.swift stop logic.
    }

    public func forget(deviceId: Device.ID) async {
        // TODO(Phase 5): drop peripheral + persist.
    }
}
