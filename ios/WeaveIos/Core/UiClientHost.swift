import Foundation
import Observation

/// Wraps the UniFFI `UiClient` and exposes its `/ws/ui` snapshot + events
/// as observable SwiftUI state. Implements `UiEventSink` so the Rust WS
/// loop can push frames here; all state mutation happens on MainActor.
///
/// Lives for the app lifetime alongside `BleBridge`.
@MainActor
@Observable
final class UiClientHost {
    private(set) var connected: Bool = false
    private(set) var snapshot: UiSnapshot = .empty
    /// Non-fatal decode / connect errors surfaced to the UI.
    private(set) var lastError: String?

    private var client: UiClient?
    private var sink: UiClientSink?
    /// Last `connect(url)` we attempted. Used for reconnect-on-url-change.
    private(set) var activeURL: String?

    func connect(serverURL: String) async {
        guard !serverURL.isEmpty else { return }
        // Tear down previous session if the URL changed.
        if activeURL == serverURL, client != nil { return }
        await disconnect()

        let sink = UiClientSink(host: self)
        self.sink = sink
        do {
            let client = try await UiClient.connect(
                serverUrl: serverURL,
                sink: sink
            )
            self.client = client
            self.activeURL = serverURL
            lastError = nil
        } catch {
            lastError = String(describing: error)
        }
    }

    func disconnect() async {
        if let client = self.client {
            await client.shutdown()
        }
        self.client = nil
        self.sink = nil
        self.activeURL = nil
        self.connected = false
    }

    // Callback hooks (invoked from the sink on MainActor).
    fileprivate func applyConnection(_ connected: Bool) {
        self.connected = connected
    }

    fileprivate func applyFrame(_ raw: String) {
        guard let data = raw.data(using: .utf8) else { return }
        do {
            let frame = try UiFrame.decode(from: data)
            apply(frame)
        } catch {
            lastError = "frame decode: \(error)"
        }
    }

    private func apply(_ frame: UiFrame) {
        switch frame {
        case .snapshot(let s):
            snapshot = s
        case .edgeOnline(let edge):
            var edges = snapshot.edges
            if let i = edges.firstIndex(where: { $0.edgeId == edge.edgeId }) {
                edges[i] = edge
            } else {
                edges.append(edge)
            }
            snapshot.edges = edges
        case .edgeOffline(let id):
            snapshot.edges = snapshot.edges.map { e in
                var e = e
                if e.edgeId == id { e.online = false }
                return e
            }
        case .serviceState(let entry):
            var list = snapshot.serviceStates
            if let i = list.firstIndex(where: { $0.id == entry.id }) {
                list[i] = entry
            } else {
                list.append(entry)
            }
            snapshot.serviceStates = list
        case .deviceState(let entry):
            var list = snapshot.deviceStates
            if let i = list.firstIndex(where: { $0.id == entry.id }) {
                list[i] = entry
            } else {
                list.append(entry)
            }
            snapshot.deviceStates = list
        case .mappingChanged(let id, let op, let mapping):
            var list = snapshot.mappings
            if op == "delete" {
                list.removeAll { $0.mappingId == id }
            } else if let mapping {
                if let i = list.firstIndex(where: { $0.mappingId == id }) {
                    list[i] = mapping
                } else {
                    list.append(mapping)
                }
            }
            snapshot.mappings = list
        case .glyphsChanged(let glyphs):
            snapshot.glyphs = glyphs
        case .unknown:
            break
        }
    }

    // Convenience accessors for views.
    var zones: [ServiceStateEntry] {
        snapshot.serviceStates.filter { $0.serviceType == "roon" && $0.property == "zone" }
    }

    var lights: [ServiceStateEntry] {
        snapshot.serviceStates.filter { $0.serviceType == "hue" }
    }
}

/// Rust-side sink. `with_foreign` gives us a protocol on the Swift side
/// that we conform to; UniFFI calls are `nonisolated` so we hop to
/// MainActor before touching observable state.
private final class UiClientSink: UiEventSink, @unchecked Sendable {
    weak var host: UiClientHost?

    init(host: UiClientHost) {
        self.host = host
    }

    func onFrameJson(json: String) {
        Task { @MainActor [weak host] in
            host?.applyFrame(json)
        }
    }

    func onConnectionChanged(connected: Bool) {
        Task { @MainActor [weak host] in
            host?.applyConnection(connected)
        }
    }
}
