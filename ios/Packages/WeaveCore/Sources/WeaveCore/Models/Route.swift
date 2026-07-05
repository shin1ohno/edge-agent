import Foundation

public enum SkipDirection: String, Codable, Hashable, Sendable {
    case next
    case prev
}

public enum RouteAction: Codable, Hashable, Sendable {
    case volume(delta: Int)
    case skip(SkipDirection)
    case toggle
    case scene(UUID)
    case shortcut(UUID)
}

public struct Route: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var input: InputType
    public var device: Device.ID
    public var service: Service.ID
    public var action: RouteAction
    public var smoothness: Double
    public var enabled: Bool

    public init(
        id: UUID = UUID(),
        input: InputType,
        device: Device.ID,
        service: Service.ID,
        action: RouteAction,
        smoothness: Double = 0.5,
        enabled: Bool = true
    ) {
        self.id = id
        self.input = input
        self.device = device
        self.service = service
        self.action = action
        self.smoothness = smoothness
        self.enabled = enabled
    }
}
