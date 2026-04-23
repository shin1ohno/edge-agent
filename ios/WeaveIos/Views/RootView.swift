@preconcurrency import SwiftUI

struct RootView: View {
    var body: some View {
        TabView {
            ConnectionsView()
                .tabItem { Label("Connections", systemImage: "network") }

            MappingsListView()
                .tabItem { Label("Mappings", systemImage: "arrow.left.arrow.right.square") }

            HomeView()
                .tabItem { Label("Nuimo", systemImage: "dot.radiowaves.left.and.right") }

            GlyphListView()
                .tabItem { Label("Glyphs", systemImage: "square.grid.3x3.square") }

            SettingsView()
                .tabItem { Label("Settings", systemImage: "gearshape") }
        }
    }
}

#Preview {
    RootView()
        .environment(BleBridge())
        .environment(SettingsStore())
}
