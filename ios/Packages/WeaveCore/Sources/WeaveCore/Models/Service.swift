import Foundation

public enum ServiceKind: String, Codable, Hashable, CaseIterable, Sendable {
    case roon
    case hue
    case homekit
    case sonos
    case shortcut
    case webhook
}

public enum ServiceStatus: Codable, Hashable, Sendable {
    case connected
    case needsAuth
    case error(String)
}

public struct Service: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var kind: ServiceKind
    public var displayName: String
    public var status: ServiceStatus

    public init(
        id: UUID = UUID(),
        kind: ServiceKind,
        displayName: String,
        status: ServiceStatus = .needsAuth
    ) {
        self.id = id
        self.kind = kind
        self.displayName = displayName
        self.status = status
    }
}
