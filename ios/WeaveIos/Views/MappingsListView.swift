@preconcurrency import SwiftUI

/// Phase 5 MVP: read-only mapping browser backed by UiClientHost's
/// snapshot. Full RoutesEditor (create / edit routes / target candidates /
/// feedback) lands in a follow-up.
struct MappingsListView: View {
    @Environment(UiClientHost.self) private var ui
    @Environment(SettingsStore.self) private var settings

    var body: some View {
        NavigationStack {
            List {
                if ui.snapshot.mappings.isEmpty {
                    Section {
                        Text("No mappings yet.")
                            .foregroundStyle(.secondary)
                    } footer: {
                        Text(footerHint)
                            .font(.caption)
                    }
                } else {
                    Section("Mappings (\(ui.snapshot.mappings.count))") {
                        ForEach(sorted) { m in
                            NavigationLink {
                                MappingDetailView(mapping: m)
                            } label: {
                                MappingRow(mapping: m)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Mappings")
            .refreshable { await refresh() }
        }
    }

    private var sorted: [MappingRecord] {
        ui.snapshot.mappings.sorted {
            if $0.active != $1.active { return $0.active && !$1.active }
            if $0.edgeId != $1.edgeId { return $0.edgeId < $1.edgeId }
            return $0.deviceId < $1.deviceId
        }
    }

    private var footerHint: String {
        if settings.serverURL.isEmpty {
            return "Set a server URL in Settings to load mappings."
        } else if !ui.connected {
            return "Waiting for server connection…"
        } else {
            return "Create mappings from weave-web for now (RoutesEditor lands in Phase 5 v2)."
        }
    }

    private func refresh() async {
        if !settings.serverURL.isEmpty {
            await ui.connect(serverURL: settings.serverURL)
        }
    }
}

private struct MappingRow: View {
    let mapping: MappingRecord

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Circle()
                    .fill(mapping.active ? Color.green : Color.secondary)
                    .frame(width: 6, height: 6)
                Text("\(mapping.deviceType):\(mapping.deviceId)")
                    .font(.body)
                Spacer()
                Text(mapping.serviceType)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(mapping.serviceTarget)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Text(mapping.edgeId)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
    }
}

private struct MappingDetailView: View {
    let mapping: MappingRecord

    var body: some View {
        Form {
            Section("Mapping") {
                LabeledRow("ID", mapping.mappingId)
                LabeledRow("Active", mapping.active ? "yes" : "no")
            }
            Section("Source") {
                LabeledRow("Edge",        mapping.edgeId)
                LabeledRow("Device type", mapping.deviceType)
                LabeledRow("Device ID",   mapping.deviceId)
            }
            Section("Destination") {
                LabeledRow("Service type", mapping.serviceType)
                LabeledRow("Target",       mapping.serviceTarget, mono: true)
            }
        }
        .navigationTitle("Mapping")
        .navigationBarTitleDisplayMode(.inline)
    }
}

private struct LabeledRow: View {
    let label: String
    let value: String
    let mono: Bool

    init(_ label: String, _ value: String, mono: Bool = false) {
        self.label = label
        self.value = value
        self.mono = mono
    }

    var body: some View {
        HStack {
            Text(label).foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(mono ? .system(.caption, design: .monospaced) : .body)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }
}
