@preconcurrency import CoreBluetooth
import Foundation
import Observation
import os

private let nuimoLogger = Logger(subsystem: "com.shin1ohno.weave.WeaveIos", category: "Nuimo")

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
        let batteryUUID = CBUUID(string: batteryLevelUuid())
        var batteryCharacteristic: CBCharacteristic?

        for ch in chars {
            if ch.uuid == ledUUID {
                ledCharacteristic = ch
            }
            if ch.uuid == batteryUUID {
                batteryCharacteristic = ch
            }
            if ch.properties.contains(.notify) {
                peripheral.setNotifyValue(true, for: ch)
            }
        }

        // Nuimo's Battery Level characteristic only notifies on level
        // change. Trigger an explicit read so the first BatteryLevel
        // event arrives at connect time — mirrors the initial read in
        // nuimo-rs/.../backend/{macos,linux}.rs.
        if let batt = batteryCharacteristic {
            peripheral.readValue(for: batt)
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
        if let error {
            // Read or notify failures (insufficient permissions, encryption
            // negotiation in progress, etc.) end up here. Log so the failure
            // is visible during debugging instead of disappearing into a
            // silent CoreBluetooth dropout.
            nuimoLogger.error(
                "didUpdateValueFor error: char=\(characteristic.uuid.uuidString, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
            )
            return
        }
        guard let data = characteristic.value else { return }
        // `CBUUID.uuidString` returns the short (16/32-bit) form for any
        // Bluetooth-assigned UUID — Battery Level (0x2A19) lands in
        // that bucket. The Rust parser expects a canonical 128-bit
        // string, so expand short-form UUIDs against the Bluetooth Base
        // UUID before handing them off.
        let charUUID = characteristic.uuid.canonical128String

        do {
            if let event = try parseNuimoNotification(charUuid: charUUID, data: data) {
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

extension CBUUID {
    /// `CBUUID.uuidString` returns the short (16- or 32-bit) form for any
    /// UUID inside the Bluetooth Base UUID range — Battery Level (`0x2A19`)
    /// is the canonical example. The Rust parser
    /// (`weave_ios_core::parse_nuimo_notification`) calls
    /// `Uuid::parse_str`, which only accepts a full 128-bit form. Expand
    /// short-form UUIDs against the Bluetooth Base UUID before serializing
    /// so the assigned-range characteristics survive the FFI hop.
    fileprivate var canonical128String: String {
        let bytes = [UInt8](self.data)
        var full: [UInt8] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
            0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB,
        ]
        switch bytes.count {
        case 16:
            full = bytes
        case 4:
            full[0] = bytes[0]; full[1] = bytes[1]
            full[2] = bytes[2]; full[3] = bytes[3]
        case 2:
            full[2] = bytes[0]; full[3] = bytes[1]
        default:
            // Unexpected length — fall through to base UUID, the caller
            // will fail to match any known characteristic and ignore.
            break
        }
        return String(
            format: "%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
            full[0], full[1], full[2], full[3],
            full[4], full[5],
            full[6], full[7],
            full[8], full[9],
            full[10], full[11], full[12], full[13], full[14], full[15]
        )
    }
}
