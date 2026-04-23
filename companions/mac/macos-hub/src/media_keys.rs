//! Media playback commands via MediaRemote private framework.
//!
//! Why not NSEvent / CGEventPost: on macOS 12+ injecting NSSystemDefined
//! events via CGEventPost is unreliable — events reach the HID tap but
//! media apps do not respond. Sequoia appears to have further tightened
//! this path even with Accessibility permission granted.
//!
//! Why MediaRemote: it is the framework that Control Center, the Touch
//! Bar, and all Apple-shipped media widgets use internally to command
//! the "Now Playing" app. The function `MRMediaRemoteSendCommand` has
//! been stable since macOS 10.10 (2014) and is used by every
//! open-source now-playing tool (`nowplaying-cli`, `MediaControl`,
//! `macos-media-controls`). It does NOT require Accessibility permission
//! and works with any media source that has registered with the
//! `MPNowPlayingInfoCenter` — Music, Spotify, Safari/Chrome audio, QuickTime,
//! Podcast.app, etc.
//!
//! Trade-off: `MediaRemote.framework` lives under
//! `/System/Library/PrivateFrameworks/` and is not a public API.
//! It is however unlikely to change — the whole macOS media experience
//! depends on its binary compatibility.
//!
//! Commands (from MRMediaRemote.h, reverse-engineered from many sources):
//!   kMRPlay             = 0
//!   kMRPause            = 1
//!   kMRTogglePlayPause  = 2
//!   kMRStop             = 3
//!   kMRNextTrack        = 4
//!   kMRPreviousTrack    = 5

#![cfg(target_os = "macos")]

use std::ffi::{c_void, CString};
use std::sync::OnceLock;

use anyhow::{bail, Result};

const RTLD_NOW: i32 = 0x2;

#[link(name = "dl", kind = "dylib")]
extern "C" {
    fn dlopen(path: *const i8, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
    fn dlerror() -> *const i8;
}

const MEDIA_REMOTE_PATH: &str =
    "/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote";
const MEDIA_REMOTE_SYMBOL: &str = "MRMediaRemoteSendCommand";

const MR_CMD_TOGGLE_PLAY_PAUSE: u32 = 2;
const MR_CMD_NEXT_TRACK: u32 = 4;
const MR_CMD_PREVIOUS_TRACK: u32 = 5;

type MRSendCommandFn = unsafe extern "C" fn(u32, *mut c_void) -> bool;

static MR_SEND_COMMAND: OnceLock<Option<MRSendCommandFn>> = OnceLock::new();

fn load_media_remote() -> Option<MRSendCommandFn> {
    *MR_SEND_COMMAND.get_or_init(|| unsafe {
        let path = CString::new(MEDIA_REMOTE_PATH).ok()?;
        let handle = dlopen(path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            let err = dlerror();
            if !err.is_null() {
                let msg = std::ffi::CStr::from_ptr(err).to_string_lossy();
                tracing::error!("dlopen MediaRemote failed: {}", msg);
            } else {
                tracing::error!("dlopen MediaRemote returned null (unknown error)");
            }
            return None;
        }
        let name = CString::new(MEDIA_REMOTE_SYMBOL).ok()?;
        let sym = dlsym(handle, name.as_ptr());
        if sym.is_null() {
            let err = dlerror();
            if !err.is_null() {
                let msg = std::ffi::CStr::from_ptr(err).to_string_lossy();
                tracing::error!("dlsym MRMediaRemoteSendCommand failed: {}", msg);
            }
            return None;
        }
        Some(std::mem::transmute::<*mut c_void, MRSendCommandFn>(sym))
    })
}

/// True if MediaRemote.framework was located and the command symbol is
/// usable. Intended for a startup log — the actual command functions
/// below call `load_media_remote` lazily, so this is purely for UX.
pub fn is_media_remote_available() -> bool {
    load_media_remote().is_some()
}

/// Kept for API compatibility with earlier CGEventPost-based builds.
/// MediaRemote does not require Accessibility permission, so always true.
pub fn is_accessibility_trusted() -> bool {
    true
}

fn send_command(cmd: u32, name: &str) -> Result<()> {
    match load_media_remote() {
        Some(f) => unsafe {
            let _result = f(cmd, std::ptr::null_mut());
            // MediaRemote returns true on "command delivered to now-playing" and
            // false when no registered app is listening; we treat both as OK so
            // that pressing play/pause with nothing playing is not an error.
            tracing::debug!("MRMediaRemoteSendCommand({}) sent", name);
            Ok(())
        },
        None => bail!(
            "MediaRemote framework could not be loaded — commands are unavailable"
        ),
    }
}

pub fn play_pause() -> Result<()> {
    send_command(MR_CMD_TOGGLE_PLAY_PAUSE, "TogglePlayPause")
}

pub fn next_track() -> Result<()> {
    send_command(MR_CMD_NEXT_TRACK, "NextTrack")
}

pub fn previous_track() -> Result<()> {
    send_command(MR_CMD_PREVIOUS_TRACK, "PreviousTrack")
}
