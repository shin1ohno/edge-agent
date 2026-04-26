@preconcurrency import CoreBluetooth
import Foundation
import Observation
import os

// UniFFI types (`NuimoEvent`, `Glyph`, `DisplayOptions`) and functions
// (`parseNuimoNotification`, `buildLedPayload`, `nuimoServiceUuid`,
// `ledMatrixUuid`) live in `Bundle/WeaveIosCore.swift`, which is part of
// this same target — no `import WeaveIosCore` is needed.

private let bleLogger = Logger(subsystem: "com.shin1ohno.weave.WeaveIos", category: "BLE")

// Mirrors `nuimo_protocol::DEVICE_NAME`. Senic Nuimo advertises this string
// in the ADV packet's local-name field but does NOT include the 128-bit
// primary service UUID there (it lives in the scan response only). iOS
// scan filters match against the ADV packet exclusively, so we scan
// unfiltered and identify by name instead — same approach the reference
// btleplug-based macOS backend takes (`nuimo-rs/.../macos.rs`).
private let nuimoDeviceName = "Nuimo"

/// Coordinates CoreBluetooth discovery, connection, and GATT I/O for Nuimo
/// peripherals. UniFFI-exposed helpers from `WeaveIosCore` turn raw
/// notification bytes into `NuimoEvent`s and encode LED payloads.
///
/// Lives for the app lifetime; owned by `WeaveIosApp`.
@Observable
final class BleBridge: NSObject {
    /// Known paired peripherals, keyed by `peripheral.identifier`.
    private(set) var devices: [UUID: NuimoDevice] = [:]

    /// Most recent events across all connected devices, newest first.
    /// Bounded to avoid unbounded growth; treat as a rolling activity feed.
    private(set) var recentEvents: [EventEntry] = []
    private let maxRecentEvents = 50

    /// `nil` until `centralManagerDidUpdateState` fires.
    private(set) var bluetoothState: CBManagerState = .unknown

    /// `true` while the central is actively scanning for Nuimos.
    private(set) var isScanning: Bool = false

    private var central: CBCentralManager!
    private let nuimoServiceUUID: CBUUID

    /// Optional sink for outbound `/ws/edge` `DeviceState` frames. Set at
    /// construction time so device connect / disconnect / battery events
    /// reach weave-server. `nil` in unit-test fixtures and in `#Preview`s.
    private weak var edgeHost: EdgeClientHost?

    init(edgeHost: EdgeClientHost? = nil) {
        // Canonical service UUID comes from the Rust core so Swift doesn't
        // drift if the GATT spec is bumped. Used post-connect during GATT
        // discovery; not used as a scan filter (see comment on
        // `nuimoDeviceName`).
        self.nuimoServiceUUID = CBUUID(string: nuimoServiceUuid())
        self.edgeHost = edgeHost
        super.init()
        self.central = CBCentralManager(
            delegate: self,
            queue: .main,
            options: [
                CBCentralManagerOptionRestoreIdentifierKey: "weave.ble.central",
                CBCentralManagerOptionShowPowerAlertKey: true,
            ]
        )
    }

    // MARK: - Public API

    func startScan() {
        guard central.state == .poweredOn, !isScanning else {
            bleLogger.debug("startScan skipped: state=\(self.central.state.rawValue) scanning=\(self.isScanning)")
            return
        }
        isScanning = true
        // `nil` services on purpose: see `nuimoDeviceName` comment above.
        central.scanForPeripherals(
            withServices: nil,
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
        bleLogger.info("startScan: unfiltered, matching by name=\(nuimoDeviceName)")
    }

    func stopScan() {
        guard isScanning else { return }
        central.stopScan()
        isScanning = false
    }

    func connect(_ peripheral: CBPeripheral) {
        let id = peripheral.identifier
        if devices[id] == nil {
            devices[id] = NuimoDevice(peripheral: peripheral, owner: self)
        }
        central.connect(peripheral)
    }

    /// Send an LED glyph to the connected peripheral. No-op if the device
    /// is not connected or hasn't discovered the LED characteristic yet.
    func writeLedPayload(_ payload: Data, to peripheralID: UUID) {
        devices[peripheralID]?.writeLed(payload)
    }

    // MARK: - Internal hooks (called by NuimoDevice)

    func record(_ event: NuimoEvent, from peripheralID: UUID) {
        let entry = EventEntry(id: UUID(), peripheralID: peripheralID, event: event, timestamp: .now)
        recentEvents.insert(entry, at: 0)
        if recentEvents.count > maxRecentEvents {
            recentEvents.removeLast(recentEvents.count - maxRecentEvents)
        }
        // Battery has its own /ws/edge property; everything else is an
        // input the routing engine + weave-web's DevicesPane consumes.
        switch event {
        case .batteryLevel(let level):
            publishBattery(peripheralID: peripheralID, level: level)
        default:
            publishInput(peripheralID: peripheralID, event: event)
        }
    }

    private func publishConnected(peripheralID: UUID, isConnected: Bool) {
        guard let edgeHost = self.edgeHost else { return }
        Task { @MainActor in
            await edgeHost.publishNuimoConnected(deviceID: peripheralID, isConnected: isConnected)
        }
    }

    private func publishBattery(peripheralID: UUID, level: UInt8) {
        guard let edgeHost = self.edgeHost else { return }
        Task { @MainActor in
            await edgeHost.publishNuimoBattery(deviceID: peripheralID, level: level)
        }
    }

    /// Forward an input event two ways:
    ///   1. publish as `DeviceState` so weave-web's DevicesPane reflects
    ///      the live input stream (existing behavior),
    ///   2. route through the Rust engine so any matching mapping
    ///      dispatches an intent on this iPad — e.g., Apple Music
    ///      play/pause via `IosMediaDispatcher`.
    private func publishInput(peripheralID: UUID, event: NuimoEvent) {
        guard let edgeHost = self.edgeHost else { return }
        Task { @MainActor in
            await edgeHost.publishNuimoInput(deviceID: peripheralID, event: event)
            await edgeHost.routeNuimoInput(deviceID: peripheralID, event: event)
        }
    }
}

// MARK: - CBCentralManagerDelegate

extension BleBridge: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        bluetoothState = central.state
        bleLogger.info("centralManagerDidUpdateState: state=\(central.state.rawValue)")
        if central.state != .poweredOn {
            isScanning = false
        }
    }

    func centralManager(
        _ central: CBCentralManager,
        willRestoreState dict: [String: Any]
    ) {
        // Peripherals CoreBluetooth kept alive across relaunches arrive here.
        guard let restored = dict[CBCentralManagerRestoredStatePeripheralsKey]
                as? [CBPeripheral] else { return }
        for p in restored where devices[p.identifier] == nil {
            devices[p.identifier] = NuimoDevice(peripheral: p, owner: self)
        }
    }

    func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        let advName = advertisementData[CBAdvertisementDataLocalNameKey] as? String
        let name = advName ?? peripheral.name
        guard name == nuimoDeviceName else {
            bleLogger.debug("didDiscover ignored: name=\(name ?? "<nil>") rssi=\(RSSI.intValue)")
            return
        }
        if devices[peripheral.identifier] == nil {
            devices[peripheral.identifier] = NuimoDevice(peripheral: peripheral, owner: self)
            bleLogger.info("didDiscover Nuimo: id=\(peripheral.identifier.uuidString) rssi=\(RSSI.intValue)")
        }
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        bleLogger.info("didConnect: id=\(peripheral.identifier.uuidString)")
        devices[peripheral.identifier]?.handleConnected()
        publishConnected(peripheralID: peripheral.identifier, isConnected: true)
    }

    func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        bleLogger.info("didDisconnect: id=\(peripheral.identifier.uuidString) error=\(error?.localizedDescription ?? "none")")
        devices[peripheral.identifier]?.handleDisconnected()
        publishConnected(peripheralID: peripheral.identifier, isConnected: false)
    }

    func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        bleLogger.error("didFailToConnect: id=\(peripheral.identifier.uuidString) error=\(error?.localizedDescription ?? "none")")
        devices[peripheral.identifier]?.handleDisconnected()
        publishConnected(peripheralID: peripheral.identifier, isConnected: false)
    }
}

// MARK: - Event feed entry

extension BleBridge {
    struct EventEntry: Identifiable, Hashable {
        let id: UUID
        let peripheralID: UUID
        let event: NuimoEvent
        let timestamp: Date
    }
}
