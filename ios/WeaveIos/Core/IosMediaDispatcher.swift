import Foundation
import MediaPlayer
import os

private let mediaLogger = Logger(
    subsystem: "com.shin1ohno.weave.WeaveIos",
    category: "Media"
)

/// Dispatches `IosMediaIntent`s to Apple Music (Music.app) via
/// `MPMusicPlayerController.systemMusicPlayer`.
///
/// iOS sandboxes programmatic remote control such that only the built-in
/// Music.app is reachable from a third-party app. Spotify, YouTube Music,
/// Podcasts, etc. cannot be controlled from this dispatcher — users
/// running those apps on the iPad should keep music control on the Mac
/// edge.
///
/// Volume / mute / brightness intents do not reach this class: the Rust
/// `IosMediaAdapter` rejects them with `IosMediaError.unsupported` before
/// the callback fires.
///
/// MediaPlayer framework calls must run on the main thread; the dispatch
/// hop is explicit because the UniFFI callback may be invoked from a
/// tokio worker thread.
final class IosMediaDispatcher: IosMediaCallback, @unchecked Sendable {
    func handleIntent(intent: IosMediaIntent) throws {
        DispatchQueue.main.sync {
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
            }
            mediaLogger.debug(
                "dispatched: \(String(describing: intent), privacy: .public)"
            )
        }
    }
}
