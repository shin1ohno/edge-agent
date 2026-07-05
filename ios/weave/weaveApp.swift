import SwiftUI
import FeatureConnections
import FeatureDevices
import FeatureHome
import FeatureServices

@main
struct WeaveApp: App {
    var body: some Scene {
        WindowGroup {
            RootTabView()
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

    var body: some View {
        TabView(selection: $selection) {
            NavigationStack {
                HomeRootView()
                    .navigationTitle("Home")
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
    }
}

#Preview {
    RootTabView()
}
