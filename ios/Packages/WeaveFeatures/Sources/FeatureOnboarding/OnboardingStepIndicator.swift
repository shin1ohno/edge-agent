import SwiftUI
import WeaveDesign

/// 4-pip step indicator shared across the onboarding flow. The current
/// step renders as an elongated 20pt capsule; the rest are 6pt dots.
/// Past + current steps tint `firingBlue`; future steps fall back to
/// system gray (matches mockup `#ios-onboard-2..4`).
struct OnboardingStepIndicator: View {
    let current: Int
    let total: Int

    var body: some View {
        HStack(spacing: 6) {
            ForEach(0..<total, id: \.self) { i in
                Capsule()
                    .fill(i <= current ? Color.firingBlue : Color(.sRGB, red: 0.82, green: 0.82, blue: 0.84, opacity: 1))
                    .frame(width: i == current ? 20 : 6, height: 6)
                    .animation(.easeInOut(duration: 0.2), value: current)
            }
        }
        .padding(.top, 12)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Step \(current + 1) of \(total)")
    }
}
