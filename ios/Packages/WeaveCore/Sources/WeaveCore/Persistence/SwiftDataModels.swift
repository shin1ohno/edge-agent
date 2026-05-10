import Foundation
import SwiftData

@Model
public final class StoredDevice {
    @Attribute(.unique) public var id: UUID
    public var name: String
    public var nickname: String?
    public var room: String?
    public var battery: Int?
    public var firmware: String?
    public var isOnline: Bool

    public init(from device: Device) {
        self.id = device.id
        self.name = device.name
        self.nickname = device.nickname
        self.room = device.room
        self.battery = device.battery
        self.firmware = device.firmware
        self.isOnline = device.isOnline
    }

    public func snapshot() -> Device {
        Device(
            id: id,
            name: name,
            nickname: nickname,
            room: room,
            battery: battery,
            firmware: firmware,
            isOnline: isOnline
        )
    }
}

@Model
public final class StoredService {
    @Attribute(.unique) public var id: UUID
    public var kindRaw: String
    public var displayName: String
    public var statusData: Data

    public init(from service: Service) throws {
        self.id = service.id
        self.kindRaw = service.kind.rawValue
        self.displayName = service.displayName
        self.statusData = try JSONEncoder().encode(service.status)
    }

    public func snapshot() throws -> Service {
        let kind = ServiceKind(rawValue: kindRaw) ?? .webhook
        let status = try JSONDecoder().decode(ServiceStatus.self, from: statusData)
        return Service(id: id, kind: kind, displayName: displayName, status: status)
    }
}

@Model
public final class StoredRoute {
    @Attribute(.unique) public var id: UUID
    public var inputRaw: String
    public var deviceId: UUID
    public var serviceId: UUID
    public var actionData: Data
    public var smoothness: Double
    public var enabled: Bool

    public init(from route: Route) throws {
        self.id = route.id
        self.inputRaw = route.input.rawValue
        self.deviceId = route.device
        self.serviceId = route.service
        self.actionData = try JSONEncoder().encode(route.action)
        self.smoothness = route.smoothness
        self.enabled = route.enabled
    }

    public func snapshot() throws -> Route {
        let input = InputType(rawValue: inputRaw) ?? .press
        let action = try JSONDecoder().decode(RouteAction.self, from: actionData)
        return Route(
            id: id,
            input: input,
            device: deviceId,
            service: serviceId,
            action: action,
            smoothness: smoothness,
            enabled: enabled
        )
    }
}
