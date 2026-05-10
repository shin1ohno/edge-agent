import Foundation

public struct Connection: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var device: Device.ID
    public var service: Service.ID
    public var routes: [Route]

    public init(
        id: UUID = UUID(),
        device: Device.ID,
        service: Service.ID,
        routes: [Route] = []
    ) {
        self.id = id
        self.device = device
        self.service = service
        self.routes = routes
    }
}
