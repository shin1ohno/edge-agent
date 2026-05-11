import SwiftUI
import WeaveCore
import WeaveDesign

/// Devices tab — Paired Nuimo inset list + "Pair a new Nuimo…" entry
/// + Edge agents section. Mirrors the hi-fi mockup `IOSDevicesList`
/// (`#ios-tab-devices`). Phase 3 uses placeholder data and routes row
/// taps to Phase 4 stub detail views.
public struct DevicesRootView: View {
    public init() {}

    public var body: some View {
        Form {
            pairedSection
            pairNewSection
            edgeAgentsSection
        }
        .navigationDestination(for: DevicesRoute.self) { route in
            switch route {
            case .device(let id):
                Text("Device detail (\(id.uuidString.prefix(8))…) — Phase 4")
                    .foregroundStyle(.secondary)
                    .navigationTitle("Device")
            }
        }
    }

    private var pairedSection: some View {
        Section {
            ForEach(PlaceholderData.devices) { device in
                NavigationLink(value: DevicesRoute.device(device.id)) {
                    IOSInsetRow(
                        title: device.nickname ?? device.name,
                        subtitle: device.isOnline
                            ? "edge-living · \(String(device.name.suffix(5)))"
                            : "last seen 4h ago",
                        value: AnyView(deviceTrailing(device))
                    ) {
                        NuimoGlyph(size: 32, tone: device.isOnline ? .firing : .plain)
                    } accessory: {
                        EmptyView()
                    }
                }
            }
        } header: {
            Text("Paired").textCase(nil)
        } footer: {
            Text("Nuimo を長押しすると pairing モードに入ります。")
        }
    }

    private var pairNewSection: some View {
        Section {
            Button {
                // TODO(Phase 4): present Pair-new-Nuimo sheet.
            } label: {
                IOSInsetRow(
                    title: "Pair a new Nuimo…",
                    subtitle: nil,
                    symbol: "plus.circle.fill",
                    color: .firingBlue
                )
                .foregroundStyle(Color.firingBlue)
            }
            .buttonStyle(.plain)
        }
    }

    private var edgeAgentsSection: some View {
        Section {
            ForEach(PlaceholderData.edgeAgents) { agent in
                IOSInsetRow(
                    title: agent.displayName,
                    subtitle: edgeAgentSubtitle(agent),
                    value: AnyView(
                        Text(agent.hostLabel)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    ),
                    symbol: "rectangle.connected.to.line.below",
                    color: agent.isConnected ? .green : Color(weaveHex: "8E8E93")
                )
            }
        } header: {
            Text("Edge agents").textCase(nil)
        } footer: {
            Text("BLE で Nuimo を捌く端末。Mac / iPhone / Watch を edge にできます。")
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
        } else {
            Text("offline")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }

    private func edgeAgentSubtitle(_ agent: EdgeAgent) -> String {
        if agent.isConnected, let events = agent.eventsPerFiveMin {
            return "connected · \(events) events / 5min"
        }
        if let lastSeen = agent.lastSeen {
            return "offline · last seen \(lastSeen)"
        }
        return agent.isConnected ? "connected" : "offline"
    }
}

/// Phase 3 navigation routes for the Devices tab. Phase 4 replaces
/// `Text(...)` destinations with real detail views.
enum DevicesRoute: Hashable {
    case device(Device.ID)
}

#Preview {
    NavigationStack {
        DevicesRootView()
            .navigationTitle("Devices")
            .navigationDestination(for: DevicesRoute.self) { route in
                switch route {
                case .device:
                    Text("Device detail — Phase 4")
                        .foregroundStyle(.secondary)
                }
            }
    }
}
