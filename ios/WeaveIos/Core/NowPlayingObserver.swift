import AVFoundation
import Foundation
import MediaPlayer
import os

private let nowPlayingLogger = Logger(
    subsystem: "com.shin1ohno.weave.WeaveIos",
    category: "NowPlaying"
)

/// Observes Apple Music's currently-playing item via
/// `MPMusicPlayerController.systemMusicPlayer` and the system output
/// volume via `AVAudioSession`, forwarding snapshots to weave-server
/// through `EdgeClient.publishNowPlaying`.
///
/// Trigger sources:
///   - `MPMusicPlayerControllerNowPlayingItemDidChange` (track change)
///   - `MPMusicPlayerControllerPlaybackStateDidChange` (play/pause/stop)
///   - `AVAudioSession.outputVolume` KVO (volume slider, hardware
///     buttons, or our own MPVolumeView slider trick)
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
    private var volumeObservation: NSKeyValueObservation?
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

        // KVO on system output volume so the Web UI sees iPad volume
        // updates from any source (hardware buttons, control center,
        // our own MPVolumeView slider trick).
        volumeObservation = AVAudioSession.sharedInstance().observe(
            \.outputVolume,
            options: [.new]
        ) { [weak self] _, _ in
            Task { @MainActor in self?.publish() }
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
        volumeObservation?.invalidate()
        volumeObservation = nil
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

        let outputVolume = Double(AVAudioSession.sharedInstance().outputVolume)
        let info = NowPlayingInfo(
            title: item?.title,
            artist: item?.artist,
            album: item?.albumTitle,
            durationSeconds: item?.playbackDuration,
            positionSeconds: position,
            state: state,
            systemVolume: outputVolume
        )

        // Map the Swift PlaybackState enum to the wire-format string the
        // Rust feedback rules consume (`"playing"` / `"paused"` /
        // `"stopped"`). Kept in lockstep with `now_playing_value` in
        // weave-ios-core::edge_client.
        let playbackString: String
        switch state {
        case .playing: playbackString = "playing"
        case .paused:  playbackString = "paused"
        case .stopped: playbackString = "stopped"
        }

        Task {
            do {
                // Three concurrent publishes (in-process feedback pump
                // sees them as separate StateUpdate frames):
                //   1. `playback`  — string for glyph rule resolution
                //   2. `volume`    — number for volume_bar rule
                //   3. `now_playing` — composite object, UI-only
                try await edgeHost.publishPlaybackState(playbackString)
                try await edgeHost.publishVolume(outputVolume * 100.0)
                try await edgeHost.publishNowPlayingSnapshot(info: info)
            } catch {
                nowPlayingLogger.error(
                    "publishNowPlaying failed: \(String(describing: error), privacy: .public)"
                )
            }
        }
    }
}
