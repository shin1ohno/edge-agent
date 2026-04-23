import Foundation
import Observation
#if os(iOS)
import UIKit
#endif

/// Persisted app settings backed by `UserDefaults`. See
/// `~/.claude/plans/purrfect-plotting-karp.md` §B-4 for the key layout.
@MainActor
@Observable
final class SettingsStore {
    var serverURL: String {
        didSet { UserDefaults.standard.set(serverURL, forKey: Keys.serverURL) }
    }

    var edgeID: String {
        didSet { UserDefaults.standard.set(edgeID, forKey: Keys.edgeID) }
    }

    /// CoreBluetooth peripheral UUIDs previously paired with Nuimos. Used to
    /// skip the discovery step on subsequent launches.
    var knownNuimoPeripheralIDs: [String] {
        didSet {
            UserDefaults.standard.set(knownNuimoPeripheralIDs, forKey: Keys.knownNuimos)
        }
    }

    init() {
        let d = UserDefaults.standard
        self.serverURL = d.string(forKey: Keys.serverURL) ?? ""
        self.edgeID = d.string(forKey: Keys.edgeID) ?? Self.defaultEdgeID()
        self.knownNuimoPeripheralIDs = d.stringArray(forKey: Keys.knownNuimos) ?? []
    }

    private static func defaultEdgeID() -> String {
        #if os(iOS)
        // Mirror the edge-agent convention: `ios-<device-name-slug>`.
        let raw = UIDevice.current.name.lowercased()
        let slug = raw.replacingOccurrences(of: " ", with: "-")
            .filter { $0.isLetter || $0.isNumber || $0 == "-" }
        return "ios-\(slug.isEmpty ? "ipad" : slug)"
        #else
        return "ios-unknown"
        #endif
    }

    private enum Keys {
        static let serverURL = "weave.server_url"
        static let edgeID = "weave.edge_id"
        static let knownNuimos = "weave.nuimo.peripherals"
    }
}
