@preconcurrency import SwiftUI

struct RootView: View {
    var body: some View {
        TabView {
            HomeView()
                .tabItem { Label("Home", systemImage: "house") }

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
