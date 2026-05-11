import SwiftUI
import WeaveCore
import WeaveDesign

/// Push view from the Services tab. Mirrors hi-fi mockup
/// `IOSServiceDetail` lines 6416-6459. Phase 4 renders static zones /
/// capabilities; Phase 5+ swaps in live `WeaveServices` adapter data.
public struct ServiceDetailView: View {
    public let serviceId: String

    @State private var selectedZoneId: String?

    public init(serviceId: String) {
        self.serviceId = serviceId
    }

    private var entry: ServiceCatalogEntry? {
        PlaceholderData.services.first(where: { $0.id == serviceId })
    }

    public var body: some View {
        if let entry {
            Form {
                heroSection(entry: entry)
                zonesSection(entry: entry)
                capabilitiesSection(entry: entry)
                Section {
                    IOSInsetRow(
                        title: "Reauthorize",
                        symbol: "arrow.clockwise",
                        color: .firingBlue
                    )
                    IOSDestructiveRow(
                        title: "Disconnect",
                        symbol: "xmark.circle.fill",
                        alertMessage: "Disconnect \(entry.displayName)? Routes targeting this service will stop firing.",
                        confirmLabel: "Disconnect"
                    ) {
                        // TODO(Phase 5): WeaveServices.disconnect(entry.kind)
                    }
                }
            }
            .navigationTitle(entry.displayName)
            .navigationBarTitleDisplayMode(.inline)
            .onAppear {
                if selectedZoneId == nil {
                    selectedZoneId = zones(for: entry).first?.id
                }
            }
        } else {
            Text("Service not found")
                .foregroundStyle(.secondary)
        }
    }

    private func heroSection(entry: ServiceCatalogEntry) -> some View {
        Section {
            VStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(Color(weaveHex: entry.accentHex))
                    .frame(width: 80, height: 80)
                    .overlay {
                        Image(systemName: entry.sfSymbol)
                            .font(.system(size: 42, weight: .semibold))
                            .foregroundStyle(.white)
                    }
                Text(entry.displayName)
                    .font(.system(size: 24, weight: .bold))
                Text(entry.addressLine)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                HStack(spacing: 6) {
                    Circle().fill(Color.green).frame(width: 6, height: 6)
                    Text("connected · 12 ms")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.green)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(Color.green.opacity(0.18))
                .clipShape(Capsule())
            }
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity)
            .listRowBackground(Color.weaveGroupedBackground)
            .listRowInsets(EdgeInsets())
        }
    }

    @ViewBuilder
    private func zonesSection(entry: ServiceCatalogEntry) -> some View {
        let list = zones(for: entry)
        if !list.isEmpty {
            Section {
                ForEach(list) { zone in
                    Button {
                        selectedZoneId = zone.id
                    } label: {
                        HStack {
                            IOSIconBadge(symbol: entry.sfSymbol, color: Color(weaveHex: entry.accentHex))
                            VStack(alignment: .leading, spacing: 2) {
                                Text(zone.name)
                                    .foregroundStyle(.primary)
                                Text(zone.subtitle)
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 0)
                            Text(zone.valueLabel)
                                .font(.system(.footnote, design: .monospaced))
                                .foregroundStyle(.secondary)
                            if selectedZoneId == zone.id {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(Color.firingBlue)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                }
            } header: {
                Text(zonesHeader(for: entry)).textCase(nil)
            } footer: {
                Text("ここで選んだ target が rotate / press の宛先になります。")
            }
        }
    }

    @ViewBuilder
    private func capabilitiesSection(entry: ServiceCatalogEntry) -> some View {
        let caps = capabilities(for: entry)
        if !caps.isEmpty {
            Section {
                ForEach(caps, id: \.self) { cap in
                    IOSInsetRow(
                        title: cap,
                        symbol: "checkmark.circle.fill",
                        color: .green
                    )
                }
            } header: {
                Text("Capabilities").textCase(nil)
            } footer: {
                Text("この service が受け取れる intent。")
            }
        }
    }

    private struct ServiceZone: Identifiable, Hashable {
        let id: String
        let name: String
        let subtitle: String
        let valueLabel: String
    }

    private func zones(for entry: ServiceCatalogEntry) -> [ServiceZone] {
        switch entry.id {
        case "roon":
            return [
                ServiceZone(id: "living",  name: "Living",  subtitle: "\u{25B6} Now playing", valueLabel: "55"),
                ServiceZone(id: "bedroom", name: "Bedroom", subtitle: "paused",                valueLabel: "32"),
                ServiceZone(id: "kitchen", name: "Kitchen", subtitle: "idle",                  valueLabel: "0"),
            ]
        case "hue":
            return [
                ServiceZone(id: "sofa",    name: "Sofa lamp",     subtitle: "on · 74%", valueLabel: "74"),
                ServiceZone(id: "reading", name: "Reading light", subtitle: "on · 88%", valueLabel: "88"),
                ServiceZone(id: "hallway", name: "Hallway",       subtitle: "off",      valueLabel: "0"),
            ]
        default:
            return []
        }
    }

    private func zonesHeader(for entry: ServiceCatalogEntry) -> String {
        switch entry.id {
        case "roon": return "Zones"
        case "hue": return "Lights"
        default: return "Targets"
        }
    }

    private func capabilities(for entry: ServiceCatalogEntry) -> [String] {
        switch entry.id {
        case "roon":
            return ["volume", "play pause", "next track", "prev track"]
        case "hue":
            return ["brightness", "toggle", "color temp"]
        case "midi":
            return ["cc", "note on", "note off"]
        default:
            return []
        }
    }
}

#Preview {
    NavigationStack {
        ServiceDetailView(serviceId: "roon")
    }
}
