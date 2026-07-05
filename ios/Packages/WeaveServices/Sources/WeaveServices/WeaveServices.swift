import Foundation
import WeaveCore

/// Adapter surface for service integrations (Roon, Hue, HomeKit, Sonos,
/// Shortcuts, Webhooks). Each `ServiceKind` will get a concrete adapter
/// implementing this protocol in later phases.
public protocol ServiceAdapter: Sendable {
    var kind: ServiceKind { get }
    func currentStatus() async -> ServiceStatus
    func execute(_ action: RouteAction) async throws
}

/// Phase 1 placeholder. Real adapters land alongside the Services tab
/// implementation in Phase 3.
public struct StubServiceAdapter: ServiceAdapter {
    public let kind: ServiceKind

    public init(kind: ServiceKind) {
        self.kind = kind
    }

    public func currentStatus() async -> ServiceStatus {
        .needsAuth
    }

    public func execute(_ action: RouteAction) async throws {
        // TODO: real dispatch lands in WeaveServices follow-up.
    }
}
