import SwiftUI
import WeaveDesign

/// Step 1 / 4. Title + 3 feature bullets + Continue CTA. Mockup
/// reference: `#ios-onboard-1` (mockup component `IOSOnboardingWelcome`).
struct OnboardingWelcomeView: View {
    var onNext: () -> Void

    private struct Bullet: Hashable {
        let symbol: String
        let title: String
        let subtitle: String
    }

    private let bullets: [Bullet] = [
        Bullet(symbol: "square.grid.2x2",
               title: "Pair physical Nuimos",
               subtitle: "Bluetooth で繋いで、即操作"),
        Bullet(symbol: "link",
               title: "Connect to anything",
               subtitle: "Roon, Hue, MIDI, HTTP …"),
        Bullet(symbol: "hand.tap",
               title: "Feel every turn",
               subtitle: "rotate に同期した触覚"),
    ]

    var body: some View {
        VStack(spacing: 0) {
            Spacer()

            NuimoGlyph(size: 120, tone: .firing)
                .padding(.bottom, 24)

            Text("Welcome to weave")
                .font(.largeTitle.bold())
                .multilineTextAlignment(.center)

            Text("Your Nuimo controllers, woven into the things you love — Roon, Hue, Sonos, anything.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.top, 8)
                .padding(.horizontal, 28)

            VStack(spacing: 14) {
                ForEach(bullets, id: \.self) { bullet in
                    HStack(alignment: .top, spacing: 12) {
                        ZStack {
                            Circle()
                                .fill(Color.firingOrange)
                                .frame(width: 36, height: 36)
                            Image(systemName: bullet.symbol)
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.white)
                        }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(bullet.title)
                                .font(.subheadline.weight(.semibold))
                            Text(bullet.subtitle)
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                    }
                }
            }
            .padding(.top, 32)
            .padding(.horizontal, 24)

            Spacer()

            Button("Continue", action: onNext)
                .weaveCTA()
                .padding(.horizontal, 24)
                .padding(.bottom, 32)
        }
    }
}

#Preview {
    OnboardingWelcomeView(onNext: {})
}
