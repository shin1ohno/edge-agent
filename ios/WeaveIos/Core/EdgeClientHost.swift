import Foundation
import Observation
import UIKit
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
    /// Lazily created dispatcher for `service_type = "ios_media"`. Held
    /// here (not on the Rust side) because UniFFI callbacks need a Swift
    /// owner that outlives them.
    private let iosMediaDispatcher = IosMediaDispatcher()
    /// Observes Apple Music Now Playing and forwards snapshots to
    /// weave-server. Lazily initialised on first `connect`.
    private var nowPlayingObserver: NowPlayingObserver?
    /// Weak reference to the BleBridge passed via `attachBleBridge` so
    /// `LedFeedbackBridge` (built on connect) can resolve device-id
    /// strings to actual peripherals for the write call.
    private weak var bleBridge: BleBridge?
    /// LED feedback sink registered with the Rust feedback pump on
    /// connect. Held so it survives the Rust callback's lifetime.
    private var ledFeedbackBridge: LedFeedbackBridge?
    private(set) var activeURL: String?
    private(set) var activeEdgeID: String?

    /// `device_type` value used for `publish_device_state` calls from
    /// `BleBridge`. Mirrors `nuimo_protocol::DEVICE_NAME` lowercased.
    private let nuimoDeviceType = "nuimo"

    /// Capability strings included in the `Hello` frame. The iPad edge
    /// hosts a Nuimo over BLE and dispatches `ios_media` intents to
    /// Music.app via `IosMediaDispatcher`.
    private let capabilities = ["nuimo:ble", "ios_media"]

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
            client.registerIosMediaCallback(callback: self.iosMediaDispatcher)
            if let ble = self.bleBridge {
                let bridge = LedFeedbackBridge(ble: ble)
                self.ledFeedbackBridge = bridge
                client.registerLedFeedbackCallback(sink: bridge)
            } else {
                edgeLogger.warning(
                    "BleBridge not attached — LED feedback will be silent until attachBleBridge() is called"
                )
            }
            let observer = self.nowPlayingObserver ?? NowPlayingObserver(edgeHost: self)
            self.nowPlayingObserver = observer
            observer.start()
            edgeLogger.info("EdgeClient connect requested: edgeID=\(edgeID, privacy: .public) url=\(serverURL, privacy: .public)")
        } catch {
            lastError = String(describing: error)
            edgeLogger.error("EdgeClient connect failed: \(String(describing: error), privacy: .public)")
        }
    }

    func disconnect() async {
        nowPlayingObserver?.stop()
        if let client = self.client {
            await client.shutdown()
        }
        self.client = nil
        self.sink = nil
        self.activeURL = nil
        self.activeEdgeID = nil
        self.connected = false
    }

    /// Forward a NowPlayingInfo snapshot to weave-server. No-op when not
    /// connected; throws on outbox failures so the observer can log.
    func publishNowPlayingSnapshot(info: NowPlayingInfo) async throws {
        guard let client = self.client else { return }
        try await client.publishNowPlaying(info: info)
    }

    /// Plumb the SwiftUI scene's UIWindow into the iOS-media dispatcher
    /// so the embedded `MPVolumeView` can take effect for volume / mute
    /// intents. Idempotent — the dispatcher only attaches on the first
    /// non-nil window.
    func attachIosMediaVolumeView(to window: UIWindow) {
        iosMediaDispatcher.attachVolumeView(to: window)
    }

    /// Hand the BleBridge in so `LedFeedbackBridge` (built on connect)
    /// can resolve device-id strings to peripherals when the Rust
    /// feedback pump fires. Called from `WeaveIosApp` once both
    /// `BleBridge` and `EdgeClientHost` exist.
    func attachBleBridge(_ ble: BleBridge) {
        self.bleBridge = ble
    }

    /// Publish iPad playback state (`"playing"` / `"paused"` /
    /// `"stopped"`) for `service_type = "ios_media"`. Drives both the
    /// Web UI's connection card and the local feedback pump.
    func publishPlaybackState(_ state: String) async throws {
        guard let client = self.client else { return }
        try await client.publishPlayback(state: state)
    }

    /// Publish iPad system volume (0..=100 percentage). Drives the
    /// Web UI volume bar and the local feedback pump's volume_bar
    /// rule.
    func publishVolume(_ value: Double) async throws {
        guard let client = self.client else { return }
        try await client.publishVolume(value: value)
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

    /// Route a Nuimo event through the Rust routing engine. Any intent
    /// the engine produces is dispatched in-process — `service_type =
    /// "ios_media"` reaches `IosMediaDispatcher` and runs against
    /// Music.app; other service types log and skip on the iPad edge.
    /// No-op when the EdgeClient is not connected.
    func routeNuimoInput(deviceID: UUID, event: NuimoEvent) async {
        guard let client = self.client else { return }
        let id = deviceID.uuidString.lowercased()
        await client.routeNuimoEvent(
            deviceType: nuimoDeviceType,
            deviceId: id,
            event: event
        )
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
