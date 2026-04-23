@preconcurrency import SwiftUI

@main
struct WeaveIosApp: App {
    @State private var ble = BleBridge()
    @State private var settings = SettingsStore()
    @State private var ui = UiClientHost()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(ble)
                .environment(settings)
                .environment(ui)
        }
    }
}
