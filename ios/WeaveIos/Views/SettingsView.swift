@preconcurrency import SwiftUI

struct SettingsView: View {
    @Environment(SettingsStore.self) private var settings
    @Environment(BleBridge.self) private var ble
    @Environment(EdgeClientHost.self) private var edge

    var body: some View {
        @Bindable var bindable = settings

        NavigationStack {
            Form {
                Section("Server") {
                    TextField("Server URL", text: $bindable.serverURL)
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                        .autocorrectionDisabled()
                    TextField("Edge ID", text: $bindable.edgeID)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    HStack {
                        Circle()
                            .fill(edge.connected ? Color.green : Color.secondary)
                            .frame(width: 6, height: 6)
                        Text(edge.connected ? "edge online" : "edge offline")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .textCase(nil)

                Section("Paired Nuimos") {
                    if settings.knownNuimoPeripheralIDs.isEmpty {
                        Text("Pair a Nuimo from the Home tab.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(settings.knownNuimoPeripheralIDs, id: \.self) { id in
                            HStack {
                                Text(id).font(.caption).lineLimit(1)
                                Spacer()
                                Button("Forget", role: .destructive) {
                                    settings.knownNuimoPeripheralIDs.removeAll { $0 == id }
                                }
                                .controlSize(.mini)
                            }
                        }
                    }
                }

                Section("Bluetooth") {
                    Text("State: \(bleStateLabel)")
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Settings")
        }
    }

    private var bleStateLabel: String {
        switch ble.bluetoothState {
        case .poweredOn: return "powered on"
        case .poweredOff: return "powered off"
        case .resetting: return "resetting"
        case .unauthorized: return "unauthorized — grant Bluetooth in iOS Settings"
        case .unsupported: return "unsupported on this device"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }
}

#Preview {
    SettingsView()
        .environment(BleBridge())
        .environment(SettingsStore())
        .environment(EdgeClientHost())
}
