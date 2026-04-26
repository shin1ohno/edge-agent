import Foundation
import MediaPlayer
import os

private let nowPlayingLogger = Logger(
    subsystem: "com.shin1ohno.weave.WeaveIos",
    category: "NowPlaying"
)

/// Observes Apple Music's currently-playing item via
/// `MPMusicPlayerController.systemMusicPlayer` and forwards snapshots to
/// weave-server through `EdgeClient.publishNowPlaying`.
///
/// Three trigger sources:
///   - `MPMusicPlayerControllerNowPlayingItemDidChange` (track change)
///   - `MPMusicPlayerControllerPlaybackStateDidChange` (play/pause/stop)
///   - 5-second poll while playing, so the server-side position stays
///     within ±5s of reality without continuous traffic when paused.
///
/// `start()` is idempotent; calling on an already-started observer is a
/// no-op. `stop()` releases observers and the timer.
@MainActor
final class NowPlayingObserver {
    private weak var edgeHost: EdgeClientHost?
    private var pollTimer: Timer?
    private var observers: [NSObjectProtocol] = []
    private var started = false

    init(edgeHost: EdgeClientHost) {
        self.edgeHost = edgeHost
    }

    func start() {
        guard !started else { return }
        started = true
        let center = NotificationCenter.default
        let player = MPMusicPlayerController.systemMusicPlayer
        player.beginGeneratingPlaybackNotifications()

        observers.append(
            center.addObserver(
                forName: .MPMusicPlayerControllerNowPlayingItemDidChange,
                object: player,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in self?.publish() }
            }
        )
        observers.append(
            center.addObserver(
                forName: .MPMusicPlayerControllerPlaybackStateDidChange,
                object: player,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in self?.publish() }
            }
        )

        pollTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.publishIfPlaying() }
        }

        // Initial snapshot so the server has a value before the user
        // touches the player.
        publish()
        nowPlayingLogger.info("NowPlayingObserver started")
    }

    func stop() {
        guard started else { return }
        started = false
        pollTimer?.invalidate()
        pollTimer = nil
        let center = NotificationCenter.default
        for obs in observers {
            center.removeObserver(obs)
        }
        observers.removeAll()
        MPMusicPlayerController.systemMusicPlayer.endGeneratingPlaybackNotifications()
        nowPlayingLogger.info("NowPlayingObserver stopped")
    }

    private func publishIfPlaying() {
        if MPMusicPlayerController.systemMusicPlayer.playbackState == .playing {
            publish()
        }
    }

    private func publish() {
        guard let edgeHost = self.edgeHost else { return }
        let player = MPMusicPlayerController.systemMusicPlayer
        let item = player.nowPlayingItem

        let state: PlaybackState
        switch player.playbackState {
        case .stopped:
            state = .stopped
        case .paused, .interrupted:
            state = .paused
        case .playing, .seekingForward, .seekingBackward:
            state = .playing
        @unknown default:
            state = .stopped
        }

        // currentPlaybackTime returns NaN when there's no queue; clamp so
        // the wire format stays a finite number.
        let rawPosition = player.currentPlaybackTime
        let position = rawPosition.isFinite ? max(0, rawPosition) : 0

        let info = NowPlayingInfo(
            title: item?.title,
            artist: item?.artist,
            album: item?.albumTitle,
            durationSeconds: item?.playbackDuration,
            positionSeconds: position,
            state: state
        )

        Task {
            do {
                try await edgeHost.publishNowPlayingSnapshot(info: info)
            } catch {
                nowPlayingLogger.error(
                    "publishNowPlaying failed: \(String(describing: error), privacy: .public)"
                )
            }
        }
    }
}
