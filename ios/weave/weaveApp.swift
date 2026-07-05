import SwiftUI
import FeatureConnections
import FeatureDevices
import FeatureHome
import FeatureOnboarding
import FeatureServices
import FeatureSettings
import WeaveCore

@main
struct WeaveApp: App {
    @AppStorage(DefaultsKeys.onboardingCompleted) private var onboardingCompleted: Bool = false

    var body: some Scene {
        WindowGroup {
            if onboardingCompleted {
                RootTabView()
            } else {
                OnboardingFlowView(onComplete: {
                    onboardingCompleted = true
                })
            }
        }
    }
}

struct RootTabView: View {
    enum Tab: Hashable {
        case home
        case devices
        case connections
        case services
    }

    @State private var selection: Tab = .home
    @State private var showSettings: Bool = false

    var body: some View {
        TabView(selection: $selection) {
            NavigationStack {
                HomeRootView()
                    .navigationTitle("Home")
                    .toolbar {
                        ToolbarItem(placement: .topBarLeading) {
                            Button {
                                showSettings = true
                            } label: {
                                Image(systemName: "gearshape")
                            }
                            .accessibilityLabel("Settings")
                        }
                    }
            }
            .tabItem {
                Label("Home", systemImage: "house.fill")
            }
            .tag(Tab.home)

            NavigationStack {
                DevicesRootView()
                    .navigationTitle("Devices")
            }
            .tabItem {
                Label("Devices", systemImage: "dot.radiowaves.left.and.right")
            }
            .tag(Tab.devices)

            NavigationStack {
                ConnectionsRootView()
                    .navigationTitle("Connections")
            }
            .tabItem {
                Label("Connections", systemImage: "link")
            }
            .tag(Tab.connections)

            NavigationStack {
                ServicesRootView()
                    .navigationTitle("Services")
            }
            .tabItem {
                Label("Services", systemImage: "square.stack.3d.up")
            }
            .tag(Tab.services)
        }
        .sheet(isPresented: $showSettings) {
            SettingsRootView(onDismiss: { showSettings = false })
        }
    }
}

#Preview("Tab root") {
    RootTabView()
}

#Preview("Onboarding root") {
    OnboardingFlowView(onComplete: {})
}
