import SwiftUI
import WeaveCore
import WeaveDesign

/// Push view from the Connections tab. Mirrors hi-fi mockup
/// `IOSConnectionDetail` lines 6313-6378. Phase 4 ships the
/// presentation; route reordering / Active toggle persistence land in
/// Phase 5 alongside live mapping state.
public struct ConnectionDetailView: View {
    public let mappingId: Mapping.ID

    @State private var active: Bool = true
    @State private var syncHapticToWatch: Bool = true
    @State private var ledFeedback: Bool = true

    public init(mappingId: Mapping.ID) {
        self.mappingId = mappingId
    }

    private var mapping: Mapping? {
        PlaceholderData.mappings.first(where: { $0.id == mappingId })
    }

    private var device: Device? {
        guard let deviceId = mapping?.deviceId else { return nil }
        return PlaceholderData.devices.first(where: { $0.id == deviceId })
    }

    private var serviceEntry: ServiceCatalogEntry? {
        PlaceholderData.services.first(where: { $0.id == mapping?.serviceType })
    }

    public var body: some View {
        if let mapping {
            Form {
                heroSection(mapping: mapping)
                routesSection(mapping: mapping)
                livePreviewSection(mapping: mapping)
                togglesSection
                deleteSection(mapping: mapping)
            }
            .navigationTitle("Connection")
            .navigationBarTitleDisplayMode(.inline)
        } else {
            Text("Mapping not found")
                .foregroundStyle(.secondary)
        }
    }

    private func heroSection(mapping: Mapping) -> some View {
        Section {
            VStack(spacing: 12) {
                HStack {
                    VStack(spacing: 8) {
                        NuimoGlyph(size: 64, tone: mapping.firing ? .firing : .plain)
                        Text(device?.nickname ?? "?")
                            .font(.subheadline.weight(.semibold))
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "arrow.right")
                        .font(.title3.weight(.semibold))
                        .foregroundStyle(Color.firingOrange)
                    Spacer(minLength: 0)
                    VStack(spacing: 8) {
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(serviceColor)
                            .frame(width: 64, height: 64)
                            .overlay {
                                Image(systemName: serviceEntry?.sfSymbol ?? "play.fill")
                                    .font(.system(size: 28, weight: .semibold))
                                    .foregroundStyle(.white)
                            }
                        Text(mapping.serviceDisplayLabel)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(1)
                    }
                }

                if mapping.firing {
                    HStack(spacing: 8) {
                        Image(systemName: "bolt.fill")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Color.firingOrange)
                        Text("firing · rotate → volume +3")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(Color.firingOrange)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .background(Color.firingOrange.opacity(0.1))
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
            }
            .padding(.vertical, 8)
            .listRowBackground(Color.weaveGroupedBackground)
            .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
        }
    }

    private var serviceColor: Color {
        if let hex = serviceEntry?.accentHex {
            return Color(weaveHex: hex)
        }
        return .firingOrange
    }

    private func routesSection(mapping: Mapping) -> some View {
        Section {
            ForEach(mapping.routes) { route in
                HStack {
                    IOSIconBadge(symbol: inputSymbol(route.input), color: inputColor(route.input))
                    HStack(spacing: 6) {
                        Text(route.input.rawValue)
                            .font(.body)
                        Image(systemName: "arrow.right")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        Text(actionLabel(route.action))
                            .font(.system(.body, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.caption2)
                        .foregroundStyle(Color(weaveHex: "C7C7CC"))
                }
                .padding(.vertical, 2)
                .contentShape(Rectangle())
            }
            Button {
                // TODO(Phase 5): present input/intent picker sheet.
            } label: {
                HStack {
                    IOSIconBadge(symbol: "plus", color: .firingBlue)
                    Text("Add route…")
                        .foregroundStyle(Color.firingBlue)
                    Spacer(minLength: 0)
                }
            }
            .buttonStyle(.plain)
        } header: {
            Text("Routes").textCase(nil)
        } footer: {
            Text("rotate を tap してスケール (1, 2, 3) や intent を変更。")
        }
    }

    private func livePreviewSection(mapping: Mapping) -> some View {
        Section {
            VStack(spacing: 10) {
                Text("55")
                    .font(.system(size: 44, weight: .bold, design: .monospaced))
                    .foregroundStyle(mapping.firing ? Color.firingOrange : .primary)
                Text(mapping.serviceDisplayLabel + " · volume")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                tickBar
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
            .listRowBackground(Color.weaveCardBackground)
        } header: {
            Text("Live preview").textCase(nil)
        }
    }

    private var tickBar: some View {
        HStack(spacing: 3) {
            ForEach(0..<24, id: \.self) { i in
                Capsule()
                    .fill(i < 14 ? Color.firingOrange : Color(weaveHex: "E5E5EA"))
                    .frame(width: 2, height: 12)
            }
        }
        .padding(.top, 6)
    }

    private var togglesSection: some View {
        Section {
            Toggle(isOn: $active) {
                HStack(spacing: 12) {
                    IOSIconBadge(symbol: "bolt.fill", color: .green)
                    Text("Active")
                }
            }
            Toggle(isOn: $syncHapticToWatch) {
                HStack(spacing: 12) {
                    IOSIconBadge(symbol: "hand.tap", color: Color(weaveHex: "5856D6"))
                    Text("Sync haptic to Watch")
                }
            }
            Toggle(isOn: $ledFeedback) {
                HStack(spacing: 12) {
                    IOSIconBadge(symbol: "lightbulb.fill", color: Color(weaveHex: "FFB340"))
                    Text("LED feedback")
                }
            }
        }
    }

    private func deleteSection(mapping: Mapping) -> some View {
        Section {
            IOSDestructiveRow(
                title: "Delete connection",
                symbol: "xmark.circle.fill",
                alertMessage: "Delete \(mapping.serviceDisplayLabel)? Routes will stop firing.",
                confirmLabel: "Delete"
            ) {
                // TODO(Phase 5): remove mapping from WeaveStore.
            }
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

    private func actionLabel(_ action: RouteAction) -> String {
        switch action {
        case .volume:
            return "volume"
        case .skip(let dir):
            return "\(dir.rawValue) track"
        case .toggle:
            return "play pause"
        case .scene:
            return "scene"
        case .shortcut:
            return "shortcut"
        }
    }
}

#Preview {
    NavigationStack {
        if let id = PlaceholderData.mappings.first?.id {
            ConnectionDetailView(mappingId: id)
        }
    }
}
