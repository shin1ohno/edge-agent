import Foundation

public enum InputType: String, Codable, CaseIterable, Hashable, Sendable {
    case rotate
    case press
    case longPress
    case swipeLeft
    case swipeRight
    case swipeUp
    case swipeDown
}
