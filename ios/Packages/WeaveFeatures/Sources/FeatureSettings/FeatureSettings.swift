import SwiftUI
import WeaveCore
import WeaveDesign

/// Top-level Settings sheet content. Mirrors the hi-fi mockup
/// `#ios-settings-light` / `#ios-settings-dark` (`IPhoneSettings`).
/// Phase 2 renders the layout against `OnboardingSnapshot.placeholder`;
/// Phase 3+ wires real persistence-backed data.
public struct SettingsRootView: View {
    public var onDismiss: (() -> Void)?

    @AppStorage(DefaultsKeys.useServer) private var useServer: Bool = false
    @AppStorage(DefaultsKeys.serverURL) private var serverURL: String = ""
    @AppStorage(DefaultsKeys.batteryAlertThreshold) private var batteryAlertThreshold: Int = 20
    @AppStorage(DefaultsKeys.openAtLaunch) private var openAtLaunch: Bool = false

    private let snapshot = OnboardingSnapshot.placeholder

    public init(onDismiss: (() -> Void)? = nil) {
        self.onDismiss = onDismiss
    }

    public var body: some View {
        NavigationStack {
            Form {
                SettingsAccountSection()
                SettingsGeneralSection(openAtLaunch: $openAtLaunch)
                SettingsDevicesSection(devices: snapshot.foundDevices)
                if let sofa = snapshot.foundDevices.first {
                    SettingsQuickActionsSection(
                        device: sofa,
                        batteryAlertThreshold: $batteryAlertThreshold
                    )
                }
                SettingsServerSection(
                    useServer: $useServer,
                    serverURL: $serverURL
                )
                SettingsFooterSection()
            }
            .navigationTitle("Settings")
            .toolbar {
                if onDismiss != nil {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Done") { onDismiss?() }
                            .font(.body.weight(.semibold))
                    }
                }
            }
        }
    }
}

#Preview {
    SettingsRootView(onDismiss: {})
}
