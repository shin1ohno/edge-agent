import AVFoundation
import Foundation
import MediaPlayer
import UIKit
import os

private let mediaLogger = Logger(
    subsystem: "com.shin1ohno.weave.WeaveIos",
    category: "Media"
)

/// Dispatches `IosMediaIntent`s to:
///   - Apple Music (Music.app) for transport / seek via
///     `MPMusicPlayerController.systemMusicPlayer`. iOS sandbox limits
///     programmatic remote control to Music.app, so Spotify / YouTube
///     Music / Podcasts won't react to play/pause/next/previous from
///     this dispatcher.
///   - System volume for volume / mute / unmute via the `MPVolumeView`
///     internal `UISlider` trick (see [SubtleVolume][]). Affects every
///     app that's currently playing audio, not just Music.app — so this
///     half of the dispatcher works system-wide regardless of which
///     player is in front.
///
/// The slider trick relies on undocumented `MPVolumeView` internals
/// (the embedded UISlider). It has been stable since 2016 across iOS
/// versions but is theoretically subject to break on a future update.
///
/// All MediaPlayer / UIKit / AVAudioSession side effects are routed
/// through `@MainActor` helpers; `handleIntent` is called from the
/// UniFFI tokio worker so it hops to main via `DispatchQueue.main.sync`
/// + `MainActor.assumeIsolated` before touching state. Swift 6 strict
/// concurrency is satisfied without making the class itself
/// MainActor-bound (which would force every Rust → Swift callback to
/// be MainActor-aware).
///
/// [SubtleVolume]: https://github.com/andreamazz/SubtleVolume
final class IosMediaDispatcher: IosMediaCallback, @unchecked Sendable {
    /// Off-screen, near-invisible MPVolumeView. Lazily constructed on
    /// `attachVolumeView(to:)` because `UIView` requires MainActor for
    /// init under Swift 6 strict concurrency. `nil` until attached
    /// — volume / mute intents log + no-op in that case.
    @MainActor private var volumeView: MPVolumeView?
    /// Last non-zero volume captured at `mute` time, restored on
    /// `unmute`. `nil` means "user hasn't muted via this path", so
    /// `unmute` is a no-op.
    @MainActor private var mutedFromVolume: Float?

    init() {
        // The MPVolumeView trick requires an active audio session. The
        // ambient category lets us coexist with whatever is currently
        // playing instead of ducking it. AVAudioSession is thread-safe
        // so this runs fine off MainActor.
        do {
            try AVAudioSession.sharedInstance().setCategory(.ambient, options: [])
            try AVAudioSession.sharedInstance().setActive(true)
        } catch {
            mediaLogger.error(
                "AVAudioSession activation failed: \(String(describing: error), privacy: .public)"
            )
        }
    }

    /// Attach an `MPVolumeView` to the supplied window so the slider
    /// trick can take effect. Idempotent: a second call once we already
    /// have a superview is a no-op. Called from `WeaveIosApp` when the
    /// SwiftUI scene is on screen.
    @MainActor
    func attachVolumeView(to window: UIWindow) {
        guard volumeView == nil else { return }
        let v = MPVolumeView(frame: CGRect(x: -1000, y: -1000, width: 100, height: 100))
        v.alpha = 0.0001
        v.showsRouteButton = false
        window.addSubview(v)
        volumeView = v
        mediaLogger.info("MPVolumeView attached to window for slider-trick dispatch")
    }

    func handleIntent(intent: IosMediaIntent) throws {
        // UniFFI delivers the callback from a tokio worker thread.
        // Hop to main, then assume MainActor isolation so the
        // MainActor-bound helpers below typecheck under Swift 6 strict
        // concurrency.
        DispatchQueue.main.sync {
            MainActor.assumeIsolated {
                self.dispatchOnMain(intent: intent)
            }
        }
    }

    @MainActor
    private func dispatchOnMain(intent: IosMediaIntent) {
        let player = MPMusicPlayerController.systemMusicPlayer
        switch intent {
        case .play:
            player.play()
        case .pause:
            player.pause()
        case .playPause:
            switch player.playbackState {
            case .playing:
                player.pause()
            default:
                player.play()
            }
        case .stop:
            player.stop()
        case .next:
            player.skipToNextItem()
        case .previous:
            player.skipToPreviousItem()
        case .seekRelative(let seconds):
            player.currentPlaybackTime = max(0, player.currentPlaybackTime + seconds)
        case .seekAbsolute(let seconds):
            player.currentPlaybackTime = max(0, seconds)
        case .volumeSet(let value):
            setSystemVolume(Float(max(0, min(1, value))))
        case .volumeChange(let delta):
            // The routing engine emits delta on a 0..100 percentage
            // scale (Nuimo `damping=80` × rotate `0.05` → `delta=4`).
            // AVAudioSession is 0..1, so divide.
            let current = AVAudioSession.sharedInstance().outputVolume
            let target = max(0, min(1, current + Float(delta) / 100))
            setSystemVolume(target)
        case .mute:
            let current = AVAudioSession.sharedInstance().outputVolume
            if current > 0 {
                mutedFromVolume = current
            }
            setSystemVolume(0)
        case .unmute:
            if let restore = mutedFromVolume, restore > 0 {
                setSystemVolume(restore)
                mutedFromVolume = nil
            }
        }
        mediaLogger.debug(
            "dispatched: \(String(describing: intent), privacy: .public)"
        )
    }

    /// Drive system volume via the MPVolumeView's embedded UISlider.
    /// No-op (with a warning log) when the slider hasn't been laid out
    /// yet — typically because `attachVolumeView(to:)` was never called
    /// or fires before the first layout pass completes.
    @MainActor
    private func setSystemVolume(_ value: Float) {
        guard let slider = volumeView?.subviews.compactMap({ $0 as? UISlider }).first else {
            mediaLogger.warning(
                "MPVolumeView UISlider not found — call attachVolumeView(to:) before dispatching volume intents"
            )
            return
        }
        slider.value = value
    }
}
