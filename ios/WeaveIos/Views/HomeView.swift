@preconcurrency import SwiftUI
@preconcurrency import WeaveIosCore

/// Minimal Phase 3 surface: scan/connect controls, a recent-event feed, and
/// an LED test button. Roon control, mappings, glyph editor come in later
/// phases.
struct HomeView: View {
    @Environment(BleBridge.self) private var ble

    var body: some View {
        NavigationStack {
            List {
                Section("Bluetooth") {
                    Text("State: \(stateLabel)")
                    if ble.isScanning {
                        Button("Stop Scan") { ble.stopScan() }
                    } else {
                        Button("Scan for Nuimos") { ble.startScan() }
                            .disabled(ble.bluetoothState != .poweredOn)
                    }
                }

                Section("Devices (\(ble.devices.count))") {
                    if ble.devices.isEmpty {
                        Text("No Nuimo discovered yet.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(Array(ble.devices.values), id: \.identifier) { device in
                            DeviceRow(device: device)
                        }
                    }
                }

                Section("Recent events") {
                    if ble.recentEvents.isEmpty {
                        Text("Events will appear here once a Nuimo is connected.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(ble.recentEvents) { entry in
                            EventRow(entry: entry)
                        }
                    }
                }
            }
            .navigationTitle("Weave")
        }
    }

    private var stateLabel: String {
        switch ble.bluetoothState {
        case .poweredOn: return "powered on"
        case .poweredOff: return "powered off"
        case .resetting: return "resetting"
        case .unauthorized: return "unauthorized"
        case .unsupported: return "unsupported"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }
}

private struct DeviceRow: View {
    let device: NuimoDevice
    @Environment(BleBridge.self) private var ble

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(device.displayName).bold()
                Spacer()
                Text(connectionLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let level = device.batteryLevel {
                Text("Battery: \(level)%").font(.caption)
            }
            HStack(spacing: 12) {
                Button("Connect") { ble.connect(device.peripheral) }
                    .disabled(device.state != .disconnected)
                Button("Test LED: A") { sendTestGlyphA() }
                    .disabled(device.state != .ready)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(.vertical, 4)
    }

    private var connectionLabel: String {
        switch device.state {
        case .disconnected: return "disconnected"
        case .connecting:   return "connecting"
        case .discovering:  return "discovering"
        case .ready:        return "ready"
        }
    }

    private func sendTestGlyphA() {
        // "A" rendered as a 9x9 grid. Space = off, `*` = on.
        let ascii = """
        .........
        ...***...
        ..*...*..
        ..*...*..
        ..*****..
        ..*...*..
        ..*...*..
        ..*...*..
        .........
        """
        let rows: [UInt16] = ascii.split(separator: "\n").map { line in
            var v: UInt16 = 0
            for (i, ch) in line.enumerated() where ch == "*" && i < 9 {
                v |= (1 << i)
            }
            return v
        }
        let glyph = Glyph(rows: rows)
        let opts = DisplayOptions(
            brightness: 1.0,
            timeoutMs: 2000,
            transition: .crossFade
        )
        do {
            let payload = try buildLedPayload(glyph: glyph, opts: opts)
            ble.writeLedPayload(Data(payload), to: device.identifier)
        } catch {
            print("build LED payload failed: \(error)")
        }
    }
}

private struct EventRow: View {
    let entry: BleBridge.EventEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.body)
            Text(entry.timestamp, style: .time)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }

    private var label: String {
        switch entry.event {
        case .buttonDown: return "button down"
        case .buttonUp: return "button up"
        case .rotate(let delta, _): return String(format: "rotate %+.3f", delta)
        case .swipeLeft: return "swipe left"
        case .swipeRight: return "swipe right"
        case .swipeUp: return "swipe up"
        case .swipeDown: return "swipe down"
        case .touchLeft: return "touch left"
        case .touchRight: return "touch right"
        case .touchTop: return "touch top"
        case .touchBottom: return "touch bottom"
        case .longTouchLeft: return "long touch left"
        case .longTouchRight: return "long touch right"
        case .longTouchTop: return "long touch top"
        case .longTouchBottom: return "long touch bottom"
        case .flyLeft: return "fly left"
        case .flyRight: return "fly right"
        case .hover(let p): return String(format: "hover %.2f", p)
        case .batteryLevel(let level): return "battery \(level)%"
        }
    }
}
