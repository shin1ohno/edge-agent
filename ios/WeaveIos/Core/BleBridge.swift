@preconcurrency import CoreBluetooth
import Foundation
import Observation

// UniFFI types (`NuimoEvent`, `Glyph`, `DisplayOptions`) and functions
// (`parseNuimoNotification`, `buildLedPayload`, `nuimoServiceUuid`,
// `ledMatrixUuid`) live in `Bundle/WeaveIosCore.swift`, which is part of
// this same target — no `import WeaveIosCore` is needed.

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

    override init() {
        // Canonical service UUID comes from the Rust core so Swift doesn't
        // drift if the GATT spec is bumped.
        self.nuimoServiceUUID = CBUUID(string: nuimoServiceUuid())
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
        guard central.state == .poweredOn, !isScanning else { return }
        isScanning = true
        central.scanForPeripherals(
            withServices: [nuimoServiceUUID],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
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
    }
}

// MARK: - CBCentralManagerDelegate

extension BleBridge: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        bluetoothState = central.state
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
        if devices[peripheral.identifier] == nil {
            devices[peripheral.identifier] = NuimoDevice(peripheral: peripheral, owner: self)
        }
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        devices[peripheral.identifier]?.handleConnected()
    }

    func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        devices[peripheral.identifier]?.handleDisconnected()
    }

    func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        devices[peripheral.identifier]?.handleDisconnected()
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
