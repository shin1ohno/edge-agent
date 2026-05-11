import SwiftUI
import WeaveCore
import WeaveDesign

/// Push view from the Devices tab. Mirrors hi-fi mockup
/// `IOSDeviceDetail` lines 6220-6280. Phase 4 reads from
/// `PlaceholderData`; tests and forget are visual stubs (Phase 5 wires
/// real LED / haptic / unpair via WeaveBLE).
public struct DeviceDetailView: View {
    public let deviceId: Device.ID

    public init(deviceId: Device.ID) {
        self.deviceId = deviceId
    }

    private var device: Device? {
        PlaceholderData.devices.first(where: { $0.id == deviceId })
    }

    private var deviceMapping: Mapping? {
        PlaceholderData.mappings.first(where: { $0.deviceId == deviceId })
    }

    public var body: some View {
        if let device {
            content(device: device, mapping: deviceMapping)
        } else {
            Text("Device not found")
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func content(device: Device, mapping: Mapping?) -> some View {
        Form {
            Section {
                hero(device: device, mapping: mapping)
                    .listRowBackground(Color.weaveGroupedBackground)
                    .listRowInsets(EdgeInsets())
                IOSStatStrip(items: [
                    .init(label: "Battery", value: "\(device.battery ?? 0)%", valueColor: (device.battery ?? 0) < 20 ? .firingRed : .green),
                    .init(label: "Edge", value: edgeShort(device)),
                    .init(label: "Status", value: device.isOnline ? "live" : "offline", valueColor: device.isOnline ? .green : .secondary),
                ])
                .listRowBackground(Color.weaveGroupedBackground)
                .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 8, trailing: 16))
            }

            routesSection(mapping: mapping)
            testSection
            settingsSection(device: device)

            Section {
                IOSDestructiveRow(
                    title: "Forget this Nuimo",
                    symbol: "xmark.circle.fill",
                    alertMessage: "Forget \(device.nickname ?? device.name)? Pair it again to restore.",
                    confirmLabel: "Forget"
                ) {
                    // TODO(Phase 5): remove device from WeaveStore + cancel
                    // its CBPeripheral subscriptions.
                }
            }
        }
        .navigationTitle(device.nickname ?? device.name)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func hero(device: Device, mapping: Mapping?) -> some View {
        VStack(spacing: 10) {
            NuimoGlyph(size: 140, tone: mapping?.firing == true ? .firing : .plain)
            Text(device.nickname ?? device.name)
                .font(.system(size: 24, weight: .bold))
            Text(device.id.uuidString.prefix(13))
                .font(.system(.caption2, design: .monospaced))
                .foregroundStyle(.secondary)
            if mapping?.firing == true {
                FiringPill(tone: .firing)
                    .padding(.top, 4)
            }
        }
        .padding(.vertical, 16)
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func routesSection(mapping: Mapping?) -> some View {
        Section {
            if let mapping, !mapping.routes.isEmpty {
                ForEach(mapping.routes) { route in
                    IOSInsetRow(
                        title: route.input.rawValue,
                        subtitle: routeSubtitle(route, target: mapping.serviceDisplayLabel),
                        value: AnyView(
                            Text(actionLabel(route.action))
                                .font(.system(.footnote, design: .monospaced))
                                .foregroundStyle(.secondary)
                        ),
                        symbol: inputSymbol(route.input),
                        color: inputColor(route.input)
                    )
                }
            } else {
                IOSInsetRow(
                    title: "No routes yet",
                    subtitle: "Tap to map an input",
                    symbol: "square.and.pencil",
                    color: .firingOrange
                )
            }
        } header: {
            Text("Routes — what each input does").textCase(nil)
        }
    }

    private var testSection: some View {
        Section {
            HStack {
                IOSIconBadge(symbol: "lightbulb.fill", color: Color(weaveHex: "FFB340"))
                Text("Show \u{201C}A\u{201D} on LEDs")
                Spacer(minLength: 0)
                Button("Test") {
                    // TODO(Phase 5): WeaveBLE.sendGlyph("A")
                }
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(Color.firingBlue)
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                .buttonStyle(.plain)
            }
            HStack {
                IOSIconBadge(symbol: "hand.tap", color: Color(weaveHex: "5856D6"))
                Text("Vibrate")
                Spacer(minLength: 0)
                Button("Buzz") {
                    // TODO(Phase 5): WeaveBLE.vibrate()
                }
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(Color.firingBlue)
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                .buttonStyle(.plain)
            }
        } header: {
            Text("Test").textCase(nil)
        }
    }

    private func settingsSection(device: Device) -> some View {
        Section {
            IOSInsetRow(
                title: "Rename",
                value: AnyView(
                    Text(device.nickname ?? device.name)
                        .foregroundStyle(.secondary)
                ),
                symbol: "square.and.pencil",
                color: .firingBlue
            )
            IOSInsetRow(
                title: "Battery alert",
                value: AnyView(
                    Text("< 20%")
                        .foregroundStyle(.secondary)
                ),
                symbol: "bell.fill",
                color: .firingRed
            )
            IOSInsetRow(
                title: "Edge agent",
                value: AnyView(
                    Text(edgeShort(device))
                        .foregroundStyle(.secondary)
                ),
                symbol: "rectangle.connected.to.line.below",
                color: .green
            )
        }
    }

    private func edgeShort(_ device: Device) -> String {
        device.room ?? "—"
    }

    private func routeSubtitle(_ route: Route, target: String) -> String {
        switch route.action {
        case .volume(let delta):
            return "step \(delta)"
        case .skip:
            return "→ \(target)"
        case .toggle:
            return "→ \(target)"
        case .scene:
            return "scene"
        case .shortcut:
            return "shortcut"
        }
    }

    private func actionLabel(_ action: RouteAction) -> String {
        switch action {
        case .volume:
            return "volume"
        case .skip(let dir):
            return "\(dir.rawValue) track"
        case .toggle:
            return "toggle"
        case .scene:
            return "scene"
        case .shortcut:
            return "shortcut"
        }
    }

    private func inputSymbol(_ input: InputType) -> String {
        switch input {
        case .rotate:
            return "arrow.up.arrow.down"
        case .press:
            return "hand.tap"
        case .longPress:
            return "hand.tap.fill"
        case .swipeLeft:
            return "arrow.left"
        case .swipeRight:
            return "arrow.right"
        case .swipeUp:
            return "arrow.up"
        case .swipeDown:
            return "arrow.down"
        }
    }

    private func inputColor(_ input: InputType) -> Color {
        switch input {
        case .rotate:
            return .firingOrange
        case .press, .longPress:
            return .firingBlue
        case .swipeLeft, .swipeRight, .swipeUp, .swipeDown:
            return Color(weaveHex: "5856D6")
        }
    }
}

#Preview {
    NavigationStack {
        if let firstId = PlaceholderData.devices.first?.id {
            DeviceDetailView(deviceId: firstId)
        }
    }
}
