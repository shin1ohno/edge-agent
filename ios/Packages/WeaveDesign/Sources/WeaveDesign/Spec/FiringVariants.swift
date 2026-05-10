import SwiftUI
import WeaveCore

/// One row of the `FIRING_VARIANTS` spec table from the hi-fi mockup
/// (`#firing-hero-variants`). Adding a new firing input is purely
/// data-driven: extend `FIRING_VARIANTS` and `FiringHeroCard` picks it
/// up via the existing switch.
public struct FiringVariant: Hashable, Sendable {
    public let id: InputType
    public let bigValue: String
    public let bodyKind: BodyKind
    public let accent: Color

    public enum BodyKind: Hashable, Sendable {
        /// Horizontal progress + numeric value (rotate, dimmer).
        case bar(label: String, value: Double)
        /// Transition pill "A → B" (toggles, resets).
        case state(text: String)
        /// Sequence dots, last one elongated (swipes, multi-press).
        case pips(count: Int, label: String)
    }

    public init(id: InputType, bigValue: String, bodyKind: BodyKind, accent: Color) {
        self.id = id
        self.bigValue = bigValue
        self.bodyKind = bodyKind
        self.accent = accent
    }
}

public enum FiringVariants {
    public static let rotate = FiringVariant(
        id: .rotate,
        bigValue: "+3",
        bodyKind: .bar(label: "vol", value: 55),
        accent: .firingOrange
    )

    public static let press = FiringVariant(
        id: .press,
        bigValue: "⏸",
        bodyKind: .state(text: "Playing → Paused"),
        accent: .firingBlue
    )

    public static let swipe = FiringVariant(
        id: .swipeLeft,
        bigValue: "⏭",
        bodyKind: .pips(count: 3, label: "3 swipes"),
        accent: .firingPurple
    )

    public static let longPress = FiringVariant(
        id: .longPress,
        bigValue: "↺",
        bodyKind: .state(text: "volume → 0%"),
        accent: .firingRed
    )

    public static let all: [FiringVariant] = [rotate, press, swipe, longPress]

    public static func variant(for input: InputType) -> FiringVariant? {
        all.first(where: { $0.id == input })
    }
}
