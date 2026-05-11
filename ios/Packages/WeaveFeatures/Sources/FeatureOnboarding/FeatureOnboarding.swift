import SwiftUI
import WeaveCore

/// State-machine driving the 4-step onboarding flow (Welcome →
/// Bluetooth → Pairing → Server). Persists completion + server
/// preference to `DefaultsKeys.onboardingCompleted` / `.useServer` so
/// the app root scene branches to the tab UI on subsequent launches.
public struct OnboardingFlowView: View {
    public var onComplete: () -> Void

    @AppStorage(DefaultsKeys.useServer) private var useServer: Bool = false
    @State private var step: Step = .welcome

    public init(onComplete: @escaping () -> Void) {
        self.onComplete = onComplete
    }

    enum Step: Int, Hashable {
        case welcome
        case bluetooth
        case pairing
        case server
    }

    public var body: some View {
        Group {
            switch step {
            case .welcome:
                OnboardingWelcomeView(onNext: { advance(to: .bluetooth) })
            case .bluetooth:
                OnboardingBluetoothView(
                    onNext: { advance(to: .pairing) },
                    onSkip: { advance(to: .pairing) }
                )
            case .pairing:
                OnboardingPairingView(
                    onNext: { advance(to: .server) },
                    onSkip: { advance(to: .server) }
                )
            case .server:
                OnboardingServerView(
                    onConnect: { finish(useServer: true) },
                    onSkip: { finish(useServer: false) }
                )
            }
        }
        .background(Color(.systemBackground).ignoresSafeArea())
        .animation(.easeInOut(duration: 0.22), value: step)
    }

    private func advance(to next: Step) {
        step = next
    }

    private func finish(useServer wantsServer: Bool) {
        self.useServer = wantsServer
        onComplete()
    }
}

#Preview("Onboarding · Welcome") {
    OnboardingFlowView(onComplete: {})
}
