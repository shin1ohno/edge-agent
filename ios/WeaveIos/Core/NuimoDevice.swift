@preconcurrency import CoreBluetooth
import Foundation
import Observation

/// A single paired Nuimo peripheral. Observed by views for connection state
/// and battery.
@Observable
final class NuimoDevice: NSObject, CBPeripheralDelegate {
    enum ConnectionState: Equatable {
        case disconnected
        case connecting
        case discovering
        case ready
    }

    let peripheral: CBPeripheral
    private(set) var state: ConnectionState = .disconnected
    private(set) var batteryLevel: UInt8? = nil

    private weak var owner: BleBridge?
    private var ledCharacteristic: CBCharacteristic?

    init(peripheral: CBPeripheral, owner: BleBridge) {
        self.peripheral = peripheral
        self.owner = owner
        super.init()
        peripheral.delegate = self
    }

    var identifier: UUID { peripheral.identifier }
    var displayName: String { peripheral.name ?? "Nuimo" }

    // MARK: - Called from BleBridge

    func handleConnected() {
        state = .discovering
        peripheral.discoverServices(nil)
    }

    func handleDisconnected() {
        state = .disconnected
        ledCharacteristic = nil
    }

    func writeLed(_ payload: Data) {
        guard let char = ledCharacteristic else { return }
        peripheral.writeValue(payload, for: char, type: .withoutResponse)
    }

    // MARK: - CBPeripheralDelegate

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let services = peripheral.services else { return }
        for service in services {
            peripheral.discoverCharacteristics(nil, for: service)
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        guard let chars = service.characteristics else { return }

        let ledUUID = CBUUID(string: ledMatrixUuid())

        for ch in chars {
            if ch.uuid == ledUUID {
                ledCharacteristic = ch
            }
            if ch.properties.contains(.notify) {
                peripheral.setNotifyValue(true, for: ch)
            }
        }

        if ledCharacteristic != nil, state != .ready {
            state = .ready
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        guard let data = characteristic.value else { return }
        let bytes = Array(data)
        let charUUID = characteristic.uuid.uuidString.lowercased()

        do {
            if let event = try parseNuimoNotification(charUuid: charUUID, data: bytes) {
                if case .batteryLevel(let level) = event {
                    batteryLevel = level
                }
                owner?.record(event, from: identifier)
            }
        } catch {
            // Parse failures and unknown characteristic UUIDs fall here —
            // ignore quietly; the activity feed just won't show that frame.
            //
            // Set `WEAVE_DEBUG_BLE=1` in the scheme to log.
            if ProcessInfo.processInfo.environment["WEAVE_DEBUG_BLE"] == "1" {
                print("nuimo parse error for \(charUUID): \(error)")
            }
        }
    }
}
