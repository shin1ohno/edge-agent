import Foundation
import os

private let deviceControlLogger = Logger(
    subsystem: "com.shin1ohno.weave.WeaveIos",
    category: "DeviceControl"
)

/// UniFFI sink that the Rust WS loop calls when weave-server pushes a
/// server-driven device control frame (`DisplayGlyph` / `DeviceConnect`
/// / `DeviceDisconnect`). Resolves the device-id string into a
/// CoreBluetooth UUID and dispatches into `BleBridge` on the main
/// queue.
///
/// Holds a *weak* reference to `BleBridge` for the same reason
/// `LedFeedbackBridge` does: `EdgeClientHost` owns the BleBridge in the
/// app's natural ownership graph; the Rust side keeps an
/// `Arc<dyn DeviceControlSink>` only for the duration of the
/// connection.
///
/// `device_type` filtering is intentionally narrow: this iOS edge-agent
/// only owns Nuimo BLE peripherals. Other device types — Hue lights,
/// future BLE devices on a different host — share the WS connection
/// only when relayed by weave-server, and those should be silent on
/// this iPad.
final class DeviceControlBridge: DeviceControlSink, @unchecked Sendable {
    private weak var ble: BleBridge?

    init(ble: BleBridge) {
        self.ble = ble
    }

    func connectDevice(deviceType: String, deviceId: String) async {
        guard deviceType == "nuimo" else {
            deviceControlLogger.debug(
                "ignored connect_device for type=\(deviceType, privacy: .public)"
            )
            return
        }
        guard let uuid = UUID(uuidString: deviceId) else {
            deviceControlLogger.warning(
                "connect_device: device_id is not a UUID — \(deviceId, privacy: .public)"
            )
            return
        }
        await MainActor.run { [weak ble] in
            ble?.connect(by: uuid)
        }
    }

    func disconnectDevice(deviceType: String, deviceId: String) async {
        guard deviceType == "nuimo" else {
            deviceControlLogger.debug(
                "ignored disconnect_device for type=\(deviceType, privacy: .public)"
            )
            return
        }
        guard let uuid = UUID(uuidString: deviceId) else {
            deviceControlLogger.warning(
                "disconnect_device: device_id is not a UUID — \(deviceId, privacy: .public)"
            )
            return
        }
        await MainActor.run { [weak ble] in
            ble?.disconnect(by: uuid)
        }
    }

    func displayGlyph(
        deviceType: String,
        deviceId: String,
        pattern: String,
        brightness: Float?,
        timeoutMs: UInt32?,
        transition: String?
    ) async {
        guard deviceType == "nuimo" else {
            deviceControlLogger.debug(
                "ignored display_glyph for type=\(deviceType, privacy: .public)"
            )
            return
        }
        guard let uuid = UUID(uuidString: deviceId) else {
            deviceControlLogger.warning(
                "display_glyph: device_id is not a UUID — \(deviceId, privacy: .public)"
            )
            return
        }
        let glyph = Glyph(rows: rowsFromAscii(pattern))
        let opts = DisplayOptions(
            brightness: Double(brightness ?? 1.0),
            timeoutMs: timeoutMs ?? 2000,
            transition: parseTransition(transition) ?? .crossFade
        )
        do {
            let payload = try buildLedPayload(glyph: glyph, opts: opts)
            await MainActor.run { [weak ble] in
                ble?.writeLedPayload(Data(payload), to: uuid)
            }
        } catch {
            deviceControlLogger.error(
                "display_glyph build failed for \(uuid.uuidString, privacy: .public): \(String(describing: error), privacy: .public)"
            )
        }
    }
}

/// Parse a 9-line ASCII grid (`*` = on, anything else = off) into the
/// 9-row bitmask vector `Glyph` expects. Lines beyond 9 and pixels
/// beyond column 9 are clipped silently — matches `nuimo_protocol::Glyph::from_ascii`
/// behavior on the Rust side.
private func rowsFromAscii(_ ascii: String) -> [UInt16] {
    let lines = ascii.split(separator: "\n", omittingEmptySubsequences: false)
    var rows: [UInt16] = []
    rows.reserveCapacity(9)
    for line in lines.prefix(9) {
        var v: UInt16 = 0
        for (i, ch) in line.enumerated() where ch == "*" && i < 9 {
            v |= (1 << i)
        }
        rows.append(v)
    }
    while rows.count < 9 {
        rows.append(0)
    }
    return rows
}

/// Map the wire string the server sends (`"immediate"` / `"cross_fade"`)
/// to the UniFFI `DisplayTransition` enum. Returns `nil` for unknown
/// values so the caller can fall back to a default.
private func parseTransition(_ s: String?) -> DisplayTransition? {
    guard let s = s else { return nil }
    switch s {
    case "cross_fade": return .crossFade
    case "immediate":  return .immediate
    default:           return nil
    }
}
