@preconcurrency import SwiftUI
import UIKit

@main
struct WeaveIosApp: App {
    @State private var settings = SettingsStore()
    @State private var ui = UiClientHost()
    @State private var edge: EdgeClientHost
    @State private var ble: BleBridge

    init() {
        let edgeHost = EdgeClientHost()
        let bleBridge = BleBridge(edgeHost: edgeHost)
        // Plumb BleBridge into EdgeClientHost so the LED feedback bridge
        // (built on each `connect`) can resolve device-id strings to
        // peripherals. Held weakly inside EdgeClientHost so this doesn't
        // extend BleBridge's lifetime.
        edgeHost.attachBleBridge(bleBridge)
        _edge = State(initialValue: edgeHost)
        _ble = State(initialValue: bleBridge)
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(ble)
                .environment(settings)
                .environment(ui)
                .environment(edge)
                .onAppear {
                    // The MPVolumeView slider trick used by IosMediaDispatcher
                    // for volume / mute requires the view to be inside a
                    // UIWindow's hierarchy. Defer one runloop hop so the
                    // window is fully up before we look it up.
                    DispatchQueue.main.async {
                        guard let scene = UIApplication.shared.connectedScenes
                                .compactMap({ $0 as? UIWindowScene })
                                .first,
                              let window = scene.windows.first else { return }
                        edge.attachIosMediaVolumeView(to: window)
                    }
                }
        }
    }
}
