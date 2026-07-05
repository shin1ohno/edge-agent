import SwiftUI
import WeaveCore

/// Live-Activity-style hero row used in the "Firing now" inset group on
/// the Home tab. Mirrors the hi-fi mockup `#firing-hero-variants`
/// section — each `InputType` selects one of three body renderers
/// (`bar` / `state` / `pips`) via `FIRING_VARIANTS`.
public struct FiringHeroCard: View {
    public let variant: FiringVariant
    public let deviceName: String
    public let serviceName: String

    public init(
        variant: FiringVariant,
        deviceName: String = "sofa",
        serviceName: String = "Roon Living"
    ) {
        self.variant = variant
        self.deviceName = deviceName
        self.serviceName = serviceName
    }

    public var body: some View {
        HStack(alignment: .center, spacing: 14) {
            bigValueLabel
            VStack(alignment: .leading, spacing: 6) {
                metaRow
                bodyRow
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var bigValueLabel: some View {
        Text(variant.bigValue)
            .font(.system(size: 34, weight: .semibold, design: .rounded))
            .foregroundStyle(variant.accent)
            .frame(minWidth: 56, alignment: .center)
            .accessibilityLabel(Text("Firing value \(variant.bigValue)"))
    }

    private var metaRow: some View {
        HStack(spacing: 6) {
            Text(deviceName)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
            Text("·")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Text(serviceName)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Spacer(minLength: 0)
        }
    }

    @ViewBuilder
    private var bodyRow: some View {
        switch variant.bodyKind {
        case let .bar(label, value):
            BarRow(label: label, value: value, accent: variant.accent)
        case let .state(text):
            StateRow(text: text, accent: variant.accent)
        case let .pips(count, label):
            PipsRow(count: count, label: label, accent: variant.accent)
        }
    }
}

private struct BarRow: View {
    let label: String
    let value: Double
    let accent: Color

    var body: some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(width: 24, alignment: .leading)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(Color.secondary.opacity(0.18))
                    Capsule()
                        .fill(accent)
                        .frame(width: max(0, geo.size.width * clampedFraction))
                }
            }
            .frame(height: 6)
            Text("\(Int(value))")
                .font(.caption.monospacedDigit().weight(.semibold))
                .foregroundStyle(.primary)
        }
    }

    private var clampedFraction: CGFloat {
        CGFloat(max(0, min(1, value / 100)))
    }
}

private struct StateRow: View {
    let text: String
    let accent: Color

    var body: some View {
        Text(text)
            .font(.caption.weight(.medium))
            .foregroundStyle(.primary)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(
                Capsule().fill(accent.opacity(0.18))
            )
    }
}

private struct PipsRow: View {
    let count: Int
    let label: String
    let accent: Color

    var body: some View {
        HStack(spacing: 6) {
            HStack(spacing: 4) {
                ForEach(0..<max(0, count - 1), id: \.self) { _ in
                    Circle()
                        .fill(accent.opacity(0.5))
                        .frame(width: 6, height: 6)
                }
                Capsule()
                    .fill(accent)
                    .frame(width: 16, height: 6)
            }
            Text(label)
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
        }
    }
}

#Preview("FiringHeroCard · light", traits: .sizeThatFitsLayout) {
    VStack(spacing: 12) {
        ForEach(FiringVariants.all, id: \.id) { variant in
            FiringHeroCard(variant: variant)
                .background(Color.weaveCardBackground)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
    }
    .padding()
    .background(Color.weaveGroupedBackground)
    .environment(\.colorScheme, .light)
}

#Preview("FiringHeroCard · dark", traits: .sizeThatFitsLayout) {
    VStack(spacing: 12) {
        ForEach(FiringVariants.all, id: \.id) { variant in
            FiringHeroCard(variant: variant)
                .background(Color.weaveCardBackground)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
    }
    .padding()
    .background(Color.weaveGroupedBackground)
    .environment(\.colorScheme, .dark)
}
