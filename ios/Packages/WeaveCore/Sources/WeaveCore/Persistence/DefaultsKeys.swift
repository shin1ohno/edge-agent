import Foundation

/// Single source of truth for `UserDefaults` keys used across the app.
///
/// Always reference these constants from `@AppStorage` and direct
/// `UserDefaults` reads to keep the key strings consistent. Adding a
/// new persisted value? Add it here first, then wire `@AppStorage` to
/// the constant in the view layer.
public enum DefaultsKeys {
    /// `Bool` — `true` once the user has completed (or skipped) all 4
    /// onboarding steps. Drives the app's root scene branching.
    public static let onboardingCompleted = "weave.onboarding.completed"

    /// `Bool` — set during Onboarding step 4. `true` if the user picked
    /// "Connect & finish"; `false` if they picked "Skip — set up later"
    /// (handoff §4 mandates server is optional).
    public static let useServer = "weave.use-server"

    /// `String` — WebSocket URL the optional weave-server client should
    /// connect to. Empty when not yet configured.
    public static let serverURL = "weave.server-url"

    /// `Int` — battery percentage threshold below which Settings shows
    /// a "Battery alert" warning. Default 20.
    public static let batteryAlertThreshold = "weave.battery-alert-threshold"

    /// `Bool` — start weave at system login (Mac-only setting, surfaced
    /// in iOS Settings for parity but a no-op on iOS).
    public static let openAtLaunch = "weave.open-at-launch"
}
