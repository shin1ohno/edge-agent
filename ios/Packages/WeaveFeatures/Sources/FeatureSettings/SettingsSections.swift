import SwiftUI
import WeaveCore
import WeaveDesign

// MARK: - Account

struct SettingsAccountSection: View {
    var body: some View {
        Section {
            HStack(spacing: 12) {
                ZStack {
                    Circle()
                        .fill(LinearGradient(
                            colors: [Color.firingOrange, Color(red: 1.0, green: 0.42, blue: 0.0)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ))
                        .frame(width: 36, height: 36)
                    Text("M")
                        .font(.headline.weight(.bold))
                        .foregroundStyle(.white)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text("Manabu")
                        .font(.body.weight(.semibold))
                    Text("weave Cloud · Family · 3 devices")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
            }
            .padding(.vertical, 4)
        }
    }
}

// MARK: - General

struct SettingsGeneralSection: View {
    @Binding var openAtLaunch: Bool

    var body: some View {
        Section {
            Toggle(isOn: $openAtLaunch) {
                Label {
                    Text("Open at launch")
                } icon: {
                    iconBadge(symbol: "bolt.fill", color: .firingOrange)
                }
            }
            NavigationLink {
                Text("Menu bar visibility — Mac only")
                    .foregroundStyle(.secondary)
                    .padding()
            } label: {
                Label {
                    LabeledContent("Show in menu bar", value: "On")
                } icon: {
                    iconBadge(symbol: "square.grid.2x2", color: .firingBlue)
                }
            }
            Label {
                LabeledContent("Language", value: "日本語")
            } icon: {
                iconBadge(symbol: "globe", color: .green)
            }
        } header: {
            Text("General").textCase(nil)
        }
    }
}

// MARK: - Devices

struct SettingsDevicesSection: View {
    let devices: [Device]

    var body: some View {
        Section {
            ForEach(devices) { device in
                HStack(spacing: 12) {
                    NuimoGlyph(size: 28, tone: device.isOnline ? .firing : .plain)
                    Text(device.nickname ?? device.name)
                        .font(.body)
                    Spacer(minLength: 0)
                    deviceTrailing(device)
                }
                .padding(.vertical, 2)
            }
            Label {
                Text("Pair a new Nuimo…")
                    .foregroundStyle(Color.firingBlue)
            } icon: {
                iconBadge(symbol: "plus.circle.fill", color: .firingBlue)
            }
        } header: {
            Text("Devices").textCase(nil)
        } footer: {
            Text("物理 Nuimo の pairing と接続テスト。タップで詳細。")
        }
    }

    @ViewBuilder
    private func deviceTrailing(_ device: Device) -> some View {
        if device.isOnline, let battery = device.battery {
            HStack(spacing: 4) {
                Circle()
                    .fill(Color.green)
                    .frame(width: 6, height: 6)
                Text("\(battery)%")
                    .font(.footnote)
                    .foregroundStyle(.green)
            }
        } else if let battery = device.battery {
            Text("\(battery)%")
                .font(.footnote)
                .foregroundStyle(.secondary)
        } else {
            Text("offline")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Quick actions (selected device)

struct SettingsQuickActionsSection: View {
    let device: Device
    @Binding var batteryAlertThreshold: Int
    @State private var nickname: String

    init(device: Device, batteryAlertThreshold: Binding<Int>) {
        self.device = device
        self._batteryAlertThreshold = batteryAlertThreshold
        self._nickname = State(initialValue: device.nickname ?? device.name)
    }

    var body: some View {
        Section {
            HStack {
                Label {
                    Text("Rename")
                } icon: {
                    iconBadge(symbol: "square.and.pencil", color: .firingBlue)
                }
                Spacer(minLength: 0)
                TextField("nickname", text: $nickname)
                    .multilineTextAlignment(.trailing)
                    .foregroundStyle(.secondary)
            }
            Toggle(isOn: .constant(device.isOnline)) {
                Label {
                    Text("Connect")
                } icon: {
                    iconBadge(symbol: "rectangle.connected.to.line.below", color: .green)
                }
            }
            .disabled(true)
            HStack {
                Label {
                    Text("Show \u{201C}A\u{201D} on LEDs")
                } icon: {
                    iconBadge(symbol: "lightbulb.fill", color: .firingOrange)
                }
                Spacer(minLength: 0)
                Button("Test") {
                    // TODO(Phase 5): trigger LED test via WeaveBLE.
                }
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(Color.firingBlue)
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                .buttonStyle(.plain)
            }
            Stepper(value: $batteryAlertThreshold, in: 5...50, step: 5) {
                Label {
                    LabeledContent("Battery alert", value: "< \(batteryAlertThreshold)%")
                } icon: {
                    iconBadge(symbol: "bell.fill", color: .firingRed)
                }
            }
        } header: {
            Text("\(device.nickname ?? device.name) — Quick actions").textCase(nil)
        }
    }
}

// MARK: - Server

struct SettingsServerSection: View {
    @Binding var useServer: Bool
    @Binding var serverURL: String

    var body: some View {
        Section {
            HStack {
                Label {
                    Text("Server URL")
                } icon: {
                    iconBadge(symbol: "rectangle.connected.to.line.below", color: .green)
                }
                Spacer(minLength: 0)
                TextField("ws://…:8765", text: $serverURL)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                    .multilineTextAlignment(.trailing)
                    .font(.system(.footnote, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
            HStack {
                Label {
                    Text("Connection")
                } icon: {
                    iconBadge(symbol: "waveform", color: .firingBlue)
                }
                Spacer(minLength: 0)
                HStack(spacing: 4) {
                    Circle()
                        .fill(Color.secondary)
                        .frame(width: 6, height: 6)
                    Text("Not configured")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            Label {
                LabeledContent("Mode", value: useServer ? "Server" : "Local")
            } icon: {
                iconBadge(symbol: "arrow.up.arrow.down", color: Color(red: 0.345, green: 0.337, blue: 0.839))
            }
            Toggle(isOn: Binding(get: { !useServer }, set: { useServer = !$0 })) {
                Label {
                    Text("Use without server")
                } icon: {
                    iconBadge(symbol: "laptopcomputer", color: .secondary)
                }
            }
        } header: {
            Text("weave Server (optional)").textCase(nil)
        } footer: {
            Text("Server なしでも Nuimo は動きます。複数デバイス sync や外出先からの cloud 制御を使うときだけ繋いでください。")
        }
    }
}

// MARK: - Footer

struct SettingsFooterSection: View {
    var body: some View {
        Section {
            EmptyView()
        } footer: {
            VStack {
                Text(versionString)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 12)
        }
    }

    private var versionString: String {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? "0.0.0"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "weave \(version) · build \(build)"
    }
}

// MARK: - Helpers

@ViewBuilder
private func iconBadge(symbol: String, color: Color) -> some View {
    RoundedRectangle(cornerRadius: 6, style: .continuous)
        .fill(color)
        .frame(width: 28, height: 28)
        .overlay {
            Image(systemName: symbol)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.white)
        }
}
