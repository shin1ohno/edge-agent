import Foundation
import os

private let feedbackLogger = Logger(
    subsystem: "com.shin1ohno.weave.WeaveIos",
    category: "Feedback"
)

/// UniFFI sink that the Rust feedback pump calls when a Nuimo LED
/// frame is ready to write. Resolves the device-id string the pump
/// hands across into the matching `BleBridge` peripheral and dispatches
/// the BLE write on the main queue (CoreBluetooth must be touched on
/// the main thread).
///
/// Holds a *weak* reference to `BleBridge` so the bridge doesn't
/// extend the bridge's lifetime past the app's natural ownership graph.
/// `EdgeClientHost` owns the bridge instance; the Rust side keeps an
/// `Arc<dyn LedFeedbackSink>` for the duration of the connection.
final class LedFeedbackBridge: LedFeedbackSink, @unchecked Sendable {
    private weak var ble: BleBridge?

    init(ble: BleBridge) {
        self.ble = ble
    }

    func writeLed(deviceId: String, payload: Data) {
        guard let id = UUID(uuidString: deviceId) else {
            feedbackLogger.warning(
                "feedback dispatch: device_id is not a UUID — \(deviceId, privacy: .public)"
            )
            return
        }
        DispatchQueue.main.async { [weak ble] in
            ble?.writeLedPayload(payload, to: id)
        }
    }
}
