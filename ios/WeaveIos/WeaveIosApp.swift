@preconcurrency import SwiftUI

@main
struct WeaveIosApp: App {
    @State private var settings = SettingsStore()
    @State private var ui = UiClientHost()
    @State private var edge: EdgeClientHost
    @State private var ble: BleBridge

    init() {
        let edgeHost = EdgeClientHost()
        _edge = State(initialValue: edgeHost)
        _ble = State(initialValue: BleBridge(edgeHost: edgeHost))
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(ble)
                .environment(settings)
                .environment(ui)
                .environment(edge)
        }
    }
}
