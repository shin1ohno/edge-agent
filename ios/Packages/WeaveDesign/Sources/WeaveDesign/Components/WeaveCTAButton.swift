import SwiftUI

/// Full-width primary CTA used by onboarding (Continue / Allow / Connect)
/// and other "single primary action" surfaces. Matches the hi-fi mockup:
/// 14pt corner radius, 56pt tall, white text on `firingBlue`.
public struct WeaveCTAButtonStyle: ButtonStyle {
    public enum Variant {
        case primary
        case tertiaryText
    }

    public let variant: Variant

    public init(variant: Variant = .primary) {
        self.variant = variant
    }

    public func makeBody(configuration: Configuration) -> some View {
        switch variant {
        case .primary:
            configuration.label
                .font(.body.weight(.semibold))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(Color.firingBlue.opacity(configuration.isPressed ? 0.85 : 1))
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        case .tertiaryText:
            configuration.label
                .font(.subheadline)
                .foregroundStyle(configuration.isPressed ? Color.firingBlue : .secondary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
        }
    }
}

public extension View {
    /// Convenience for the standard onboarding primary CTA shape.
    func weaveCTA(_ variant: WeaveCTAButtonStyle.Variant = .primary) -> some View {
        buttonStyle(WeaveCTAButtonStyle(variant: variant))
    }
}

#Preview("Weave CTA buttons", traits: .sizeThatFitsLayout) {
    VStack(spacing: 12) {
        Button("Continue", action: {}).weaveCTA()
        Button("Skip — set up later", action: {}).weaveCTA(.tertiaryText)
    }
    .padding(24)
}
