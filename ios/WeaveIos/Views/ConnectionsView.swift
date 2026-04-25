@preconcurrency import SwiftUI

/// LiveConsole-equivalent overview: server connection status, edges,
/// Roon zones, Hue lights. Mappings live on their own tab (Phase 5).
struct ConnectionsView: View {
    @Environment(UiClientHost.self) private var ui
    @Environment(EdgeClientHost.self) private var edge
    @Environment(SettingsStore.self) private var settings

    var body: some View {
        NavigationStack {
            List {
                Section("Server") {
                    HStack {
                        Circle()
                            .fill(ui.connected ? Color.green : Color.secondary)
                            .frame(width: 8, height: 8)
                        Text(ui.connected ? "connected" : "disconnected")
                            .foregroundStyle(.secondary)
                            .font(.callout)
                        Spacer()
                        if settings.serverURL.isEmpty {
                            Text("set URL in Settings")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        } else {
                            Text(settings.serverURL)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                    }
                    HStack {
                        Circle()
                            .fill(edge.connected ? Color.green : Color.secondary)
                            .frame(width: 8, height: 8)
                        Text(edge.connected ? "edge online" : "edge offline")
                            .foregroundStyle(.secondary)
                            .font(.callout)
                        Spacer()
                        if !settings.edgeID.isEmpty {
                            Text("as \(settings.edgeID)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                    }
                    if let err = ui.lastError {
                        Text(err)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                    if let err = edge.lastError {
                        Text(err)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }

                Section("Edges (\(ui.snapshot.edges.count))") {
                    if ui.snapshot.edges.isEmpty {
                        Text("No edges yet.").foregroundStyle(.secondary)
                    } else {
                        ForEach(ui.snapshot.edges) { edge in
                            EdgeRow(edge: edge)
                        }
                    }
                }

                Section("Zones (\(ui.zones.count))") {
                    if ui.zones.isEmpty {
                        Text("No Roon zones yet.").foregroundStyle(.secondary)
                    } else {
                        ForEach(ui.zones) { zone in
                            ZoneRow(state: zone)
                        }
                    }
                }

                Section("Lights (\(ui.lights.count))") {
                    if ui.lights.isEmpty {
                        Text("No Hue lights yet.").foregroundStyle(.secondary)
                    } else {
                        ForEach(ui.lights) { light in
                            LightRow(state: light)
                        }
                    }
                }
            }
            .navigationTitle("Connections")
            .refreshable {
                await ui.connect(serverURL: settings.serverURL)
                await edge.connect(serverURL: settings.serverURL, edgeID: settings.edgeID)
            }
        }
        .task {
            if !settings.serverURL.isEmpty, ui.activeURL != settings.serverURL {
                await ui.connect(serverURL: settings.serverURL)
            }
            if !settings.serverURL.isEmpty, !settings.edgeID.isEmpty,
               edge.activeURL != settings.serverURL || edge.activeEdgeID != settings.edgeID {
                await edge.connect(serverURL: settings.serverURL, edgeID: settings.edgeID)
            }
        }
    }
}

private struct EdgeRow: View {
    let edge: EdgeInfo

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Circle()
                    .fill(edge.online ? Color.green : Color.secondary)
                    .frame(width: 6, height: 6)
                Text(edge.edgeId).font(.body)
                Spacer()
                Text(edge.version)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            if !edge.capabilities.isEmpty {
                Text(edge.capabilities.joined(separator: " · "))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}

private struct ZoneRow: View {
    let state: ServiceStateEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(stringTitle)
                .font(.body)
            HStack {
                Text(state.edgeId).font(.caption).foregroundStyle(.secondary)
                Text("·").foregroundStyle(.secondary)
                Text(state.target).font(.caption).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
        }
        .padding(.vertical, 2)
    }

    private var stringTitle: String {
        state.stringValue ?? state.target
    }
}

private struct LightRow: View {
    let state: ServiceStateEntry

    var body: some View {
        HStack {
            Text(state.target)
                .font(.body)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            Text(state.property).font(.caption).foregroundStyle(.secondary)
            if let v = state.doubleValue {
                Text(String(format: "%.0f", v)).font(.caption)
            } else if let b = state.boolValue {
                Text(b ? "on" : "off").font(.caption)
            } else if let s = state.stringValue {
                Text(s).font(.caption).lineLimit(1)
            }
        }
    }
}
