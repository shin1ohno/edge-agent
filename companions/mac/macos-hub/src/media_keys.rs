//! Media key injection via CGEvent system-defined events.
//!
//! macOS media keys (play/pause, next, previous on keyboards with a Touch
//! Bar or the Apple Magic Keyboard function row) are delivered as
//! `NSSystemDefined` events with subtype 8 and a packed `data1` field:
//!
//!   data1 = (keyCode << 16) | (keyState << 8)
//!
//! where `keyState = 0xA` for key-down and `0xB` for key-up.
//!
//! At the CGEvent layer we emit two events per "press": one with the
//! key-down state, one with the key-up state. Both go to the HID event tap
//! so any app with a media-key handler (Music, Spotify, Chrome PWA, etc.)
//! picks them up.
//!
//! Key codes (from IOKit's `ev_keymap.h` / `NX_KEYTYPE_*`):
//!   NX_KEYTYPE_PLAY     = 16
//!   NX_KEYTYPE_NEXT     = 17
//!   NX_KEYTYPE_PREVIOUS = 18

#![cfg(target_os = "macos")]

use std::ffi::c_void;

use anyhow::{anyhow, Result};

const NX_KEYTYPE_PLAY: u32 = 16;
const NX_KEYTYPE_NEXT: u32 = 17;
const NX_KEYTYPE_PREVIOUS: u32 = 18;

// CGEventType — kCGEventSystemDefined is not a public constant in most
// bindings, but NSSystemDefined has value 14.
const K_CG_EVENT_SYSTEM_DEFINED: u32 = 14;

// CGEventTapLocation
const K_CG_HID_EVENT_TAP: u32 = 0;

// CGEventField — kCGSystemDefinedEventSubtype and kCGEventSourceUserData
// are not exported. We set fields by numeric index:
//   kCGEventSourceUserData = 42  (u.s.data1 for system events lives here)
// However, Apple's documented path is to use NSEvent's otherEventWithType
// at AppKit level. From CoreGraphics alone, a common approach used by
// media-key tools (BetterTouchTool, SpotifyControl, etc.) is:
//
//   CGEventRef ev = CGEventCreate(NULL);
//   CGEventSetType(ev, kCGEventSystemDefined);
//   CGEventSetIntegerValueField(ev, kCGEventSourceUserData, data1);
//
// `kCGEventSourceUserData` happens to map to the right slot for the
// packed data1 payload. This is the approach used here.
//
// NOTE FOR OPERATOR: if this does not fire media-key handlers on the test
// machine, the fallback is AppKit NSEvent.otherEventWithType via the
// objc2 crate. See TODO in README.
const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;

extern "C" {
    fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    fn CGEventSetType(event: CGEventRef, event_type: u32);
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap_location: u32, event: CGEventRef);
    fn CFRelease(cf: *const c_void);
}

/// Emit a single system-defined media-key event (down OR up).
fn post_media_event(key_code: u32, key_state_down: bool) -> Result<()> {
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return Err(anyhow!("CGEventCreate returned null"));
        }
        CGEventSetType(event, K_CG_EVENT_SYSTEM_DEFINED);

        // data1 layout: (keyCode << 16) | (keyState << 8)
        // keyState is NX_KEYDOWN (0x0A) or NX_KEYUP (0x0B) shifted to byte 1.
        let key_state: u32 = if key_state_down { 0x0A } else { 0x0B };
        let data1: u32 = (key_code << 16) | (key_state << 8);

        CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, data1 as i64);
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event as *const c_void);
    }
    Ok(())
}

/// Emit a full press (down + up) of a media key.
fn press_media_key(key_code: u32) -> Result<()> {
    post_media_event(key_code, true)?;
    // Tiny gap is usually unnecessary — macOS event handlers accept
    // back-to-back down/up. Keep it simple.
    post_media_event(key_code, false)?;
    Ok(())
}

pub fn play_pause() -> Result<()> {
    press_media_key(NX_KEYTYPE_PLAY)
}

pub fn next_track() -> Result<()> {
    press_media_key(NX_KEYTYPE_NEXT)
}

pub fn previous_track() -> Result<()> {
    press_media_key(NX_KEYTYPE_PREVIOUS)
}

/// No dedicated stop key on macOS media keys; treat stop as play_pause
/// when the app is playing. For the MVP we alias it to play_pause.
pub fn stop() -> Result<()> {
    play_pause()
}
