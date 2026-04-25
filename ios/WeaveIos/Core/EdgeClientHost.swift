import Foundation
import Observation
import os

private let edgeLogger = Logger(subsystem: "com.shin1ohno.weave.WeaveIos", category: "EdgeClient")

/// Wraps the UniFFI `EdgeClient` and exposes its connection liveness as
/// observable SwiftUI state. Implements `EdgeEventSink` so the Rust WS
/// loop can push connection-state changes here; all state mutation
/// happens on MainActor.
///
/// Lives for the app lifetime alongside `UiClientHost`. Where `UiClient`
/// is the consumer (`/ws/ui`), `EdgeClient` is the producer (`/ws/edge`):
/// the iPad announces itself as an edge and pushes `DeviceState` for the
/// Nuimos it has paired locally over BLE.
@MainActor
@Observable
final class EdgeClientHost {
    private(set) var connected: Bool = false
    /// Non-fatal connect / publish errors surfaced to the UI.
    private(set) var lastError: String?

    private var client: EdgeClient?
    private var sink: EdgeClientSink?
    private(set) var activeURL: String?
    private(set) var activeEdgeID: String?

    /// `device_type` value used for `publish_device_state` calls from
    /// `BleBridge`. Mirrors `nuimo_protocol::DEVICE_NAME` lowercased.
    private let nuimoDeviceType = "nuimo"

    /// Capability strings included in the `Hello` frame. iOS-as-edge can
    /// today only host a Nuimo over BLE; it has no Roon / Hue adapters,
    /// so we advertise a single capability flag.
    private let capabilities = ["nuimo:ble"]

    func connect(serverURL: String, edgeID: String) async {
        guard !serverURL.isEmpty, !edgeID.isEmpty else { return }
        if activeURL == serverURL, activeEdgeID == edgeID, client != nil { return }
        await disconnect()

        let sink = EdgeClientSink(host: self)
        self.sink = sink
        do {
            let client = try await EdgeClient.connect(
                serverUrl: serverURL,
                edgeId: edgeID,
                capabilities: capabilities,
                sink: sink
            )
            self.client = client
            self.activeURL = serverURL
            self.activeEdgeID = edgeID
            self.lastError = nil
            edgeLogger.info("EdgeClient connect requested: edgeID=\(edgeID, privacy: .public) url=\(serverURL, privacy: .public)")
        } catch {
            lastError = String(describing: error)
            edgeLogger.error("EdgeClient connect failed: \(String(describing: error), privacy: .public)")
        }
    }

    func disconnect() async {
        if let client = self.client {
            await client.shutdown()
        }
        self.client = nil
        self.sink = nil
        self.activeURL = nil
        self.activeEdgeID = nil
        self.connected = false
    }

    /// Publish `nuimo / <id> / connected = true|false`. No-op if no client.
    func publishNuimoConnected(deviceID: UUID, isConnected: Bool) async {
        await publish(deviceID: deviceID, property: "connected", valueJSON: isConnected ? "true" : "false")
    }

    /// Publish `nuimo / <id> / battery = <level>`. No-op if no client.
    func publishNuimoBattery(deviceID: UUID, level: UInt8) async {
        await publish(deviceID: deviceID, property: "battery", valueJSON: String(level))
    }

    /// Publish `nuimo / <id> / input = { input: "<name>", … }`. No-op if
    /// the event has no `input` projection (battery, etc.) or no client.
    /// The JSON shape comes from `nuimo_input_event_json` in
    /// `weave-ios-core` so it stays in sync with the Linux edge-agent.
    func publishNuimoInput(deviceID: UUID, event: NuimoEvent) async {
        guard let json = nuimoInputEventJson(event: event) else { return }
        await publish(deviceID: deviceID, property: "input", valueJSON: json)
    }

    private func publish(deviceID: UUID, property: String, valueJSON: String) async {
        guard let client = self.client else { return }
        let id = deviceID.uuidString.lowercased()
        do {
            try await client.publishDeviceState(
                deviceType: nuimoDeviceType,
                deviceId: id,
                property: property,
                valueJson: valueJSON
            )
            edgeLogger.debug("publish DeviceState nuimo/\(id, privacy: .public)/\(property, privacy: .public)=\(valueJSON, privacy: .public)")
        } catch {
            lastError = String(describing: error)
            edgeLogger.error("publishDeviceState failed: \(String(describing: error), privacy: .public)")
        }
    }

    fileprivate func applyConnection(_ connected: Bool) {
        self.connected = connected
        edgeLogger.info("EdgeClient connection=\(connected ? "up" : "down", privacy: .public)")
    }
}

/// Rust-side sink. UniFFI calls are `nonisolated` so we hop to MainActor
/// before touching observable state.
private final class EdgeClientSink: EdgeEventSink, @unchecked Sendable {
    weak var host: EdgeClientHost?

    init(host: EdgeClientHost) {
        self.host = host
    }

    func onConnectionChanged(connected: Bool) {
        Task { @MainActor [weak host] in
            host?.applyConnection(connected)
        }
    }
}
