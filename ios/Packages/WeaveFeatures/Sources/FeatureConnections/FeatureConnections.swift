import SwiftUI
import WeaveCore
import WeaveDesign

/// Connections tab — Active mappings inset list + "New connection…"
/// row. Each active row shows the device → service title, firing /
/// lastEvent value, and a 3-chip route summary as the subtitle.
/// Mirrors the hi-fi mockup `IOSConnectionsList` (`#ios-tab-connections`).
public struct ConnectionsRootView: View {
    public init() {}

    public var body: some View {
        Form {
            activeSection
            newConnectionSection
        }
        .navigationDestination(for: ConnectionsRoute.self) { route in
            switch route {
            case .mapping(let id):
                Text("Connection detail (\(id.uuidString.prefix(8))…) — Phase 4")
                    .foregroundStyle(.secondary)
                    .navigationTitle("Connection")
            }
        }
    }

    private var activeSection: some View {
        Section {
            ForEach(PlaceholderData.mappings) { mapping in
                NavigationLink(value: ConnectionsRoute.mapping(mapping.id)) {
                    mappingRow(mapping)
                }
            }
        } header: {
            Text("Active").textCase(nil)
        }
    }

    private var newConnectionSection: some View {
        Section {
            Button {
                // TODO(Phase 4): present new-connection picker sheet.
            } label: {
                IOSInsetRow(
                    title: "New connection…",
                    subtitle: nil,
                    symbol: "plus.circle.fill",
                    color: .firingBlue
                )
                .foregroundStyle(Color.firingBlue)
            }
            .buttonStyle(.plain)
        }
    }

    private func mappingRow(_ mapping: Mapping) -> some View {
        let device = PlaceholderData.devices.first(where: { $0.id == mapping.deviceId })
        return HStack(spacing: 12) {
            NuimoGlyph(size: 32, tone: mapping.firing ? .firing : .plain)
            VStack(alignment: .leading, spacing: 4) {
                Text("\(device?.nickname ?? "?") → \(mapping.serviceDisplayLabel)")
                    .font(.body)
                    .foregroundStyle(.primary)
                routeChipStrip(mapping.routes)
            }
            Spacer(minLength: 0)
            if mapping.firing {
                FiringPill(tone: .firing)
            } else {
                FiringPill(tone: .idle(mapping.lastEvent))
            }
        }
        .padding(.vertical, 2)
    }

    private func routeChipStrip(_ routes: [Route]) -> some View {
        HStack(spacing: 4) {
            ForEach(routes.prefix(3), id: \.id) { route in
                Text(routeChipLabel(route))
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(Color.black.opacity(0.06))
                    .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
            }
        }
    }

    private func routeChipLabel(_ route: Route) -> String {
        "\(route.input.rawValue)→\(actionLabel(route.action))"
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
}

enum ConnectionsRoute: Hashable {
    case mapping(Mapping.ID)
}

#Preview {
    NavigationStack {
        ConnectionsRootView()
            .navigationTitle("Connections")
            .navigationDestination(for: ConnectionsRoute.self) { route in
                switch route {
                case .mapping:
                    Text("Connection detail — Phase 4")
                        .foregroundStyle(.secondary)
                }
            }
    }
}
