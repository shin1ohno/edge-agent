import SwiftUI
import WeaveDesign

/// Step 2 / 4. Bluetooth permission primer. The real `CBCentralManager`
/// initialisation lands in Phase 5; for now the buttons advance the
/// flow without actually triggering the system permission dialog.
/// Mockup reference: `#ios-onboard-2` / `IOSOnboardingBluetooth`.
struct OnboardingBluetoothView: View {
    var onNext: () -> Void
    var onSkip: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            OnboardingStepIndicator(current: 1, total: 4)

            Spacer()

            ZStack(alignment: .bottomTrailing) {
                RoundedRectangle(cornerRadius: 28, style: .continuous)
                    .fill(Color.firingBlue)
                    .frame(width: 112, height: 112)
                    .overlay {
                        Image(systemName: "antenna.radiowaves.left.and.right")
                            .font(.system(size: 56, weight: .semibold))
                            .foregroundStyle(.white)
                    }
                Circle()
                    .fill(.white)
                    .frame(width: 40, height: 40)
                    .overlay {
                        Image(systemName: "bolt.fill")
                            .font(.body.weight(.semibold))
                            .foregroundStyle(Color.firingOrange)
                    }
                    .shadow(color: .black.opacity(0.12), radius: 6, x: 0, y: 2)
                    .offset(x: 8, y: 8)
            }

            Text("Allow Bluetooth")
                .font(.system(size: 28, weight: .bold))
                .padding(.top, 28)

            Text("weave needs Bluetooth to detect Nuimo controllers in the room.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.top, 8)
                .padding(.horizontal, 32)

            permissionPreviewCard
                .padding(.top, 32)
                .padding(.horizontal, 24)

            Spacer()

            VStack(spacing: 2) {
                Button("Allow & Continue", action: onNext)
                    .weaveCTA()
                Button("Skip for now", action: onSkip)
                    .weaveCTA(.tertiaryText)
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 32)
        }
    }

    /// Mock of the iOS system permission alert — used as illustration
    /// during the primer step. The real alert fires from
    /// `CBCentralManager` in Phase 5.
    private var permissionPreviewCard: some View {
        VStack(spacing: 0) {
            VStack(spacing: 4) {
                Text("\u{201C}weave\u{201D} Would Like to Use Bluetooth")
                    .font(.subheadline.weight(.semibold))
                    .multilineTextAlignment(.center)
                Text("This lets weave find and connect to your Nuimo controllers.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)

            Divider()

            HStack(spacing: 0) {
                Text("Don\u{2019}t Allow")
                    .frame(maxWidth: .infinity)
                    .foregroundStyle(Color.firingBlue)
                Divider()
                    .frame(height: 32)
                Text("Allow")
                    .frame(maxWidth: .infinity)
                    .foregroundStyle(Color.firingBlue)
                    .font(.body.weight(.semibold))
            }
            .padding(.vertical, 10)
        }
        .background(Color.weaveCardBackground)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.black.opacity(0.08), lineWidth: 0.5)
        }
        .shadow(color: .black.opacity(0.08), radius: 12, x: 0, y: 2)
    }
}

#Preview {
    OnboardingBluetoothView(onNext: {}, onSkip: {})
}
