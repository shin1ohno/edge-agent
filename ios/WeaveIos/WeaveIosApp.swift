@preconcurrency import SwiftUI

@main
struct WeaveIosApp: App {
    // BleBridge owns CoreBluetooth lifecycle + restoration; lives for the app.
    @State private var ble = BleBridge()
    @State private var settings = SettingsStore()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(ble)
                .environment(settings)
        }
    }
}
