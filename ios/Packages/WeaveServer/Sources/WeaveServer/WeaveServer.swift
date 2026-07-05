import Foundation
import WeaveCore

/// Optional weave-server uplink (REST + WebSocket). Per the handoff §4,
/// the app boots in "no server" mode by default — the Onboarding step 4
/// is skippable, and Settings → Server lets the user opt in later.
public struct WeaveServerConfig: Codable, Hashable, Sendable {
    public var baseURL: URL
    public var token: String?

    public init(baseURL: URL, token: String? = nil) {
        self.baseURL = baseURL
        self.token = token
    }
}

/// Client surface for the optional weave-server. Phase 1 is a stub — the
/// real REST + WebSocket plumbing arrives once the Server pane in
/// Settings is implemented.
public protocol WeaveServerClient: AnyObject, Sendable {
    var config: WeaveServerConfig { get }
    func ping() async throws -> Bool
}

public final class StubWeaveServerClient: WeaveServerClient, @unchecked Sendable {
    public let config: WeaveServerConfig

    public init(config: WeaveServerConfig) {
        self.config = config
    }

    public func ping() async throws -> Bool {
        false
    }
}
