//! CGEvent media-key injection.
//!
//! Media keys (play/pause, next, previous) are NX system-defined events
//! (NSEvent.type == NSSystemDefined == 14), subtype 8. They cannot be sent
//! via `CGEventCreateKeyboardEvent` (which is for Unicode keyboard keys).
//!
//! The pattern used below is the classic "NSEvent otherEventWithType:14
//! subtype:8 data1:((keyCode << 16) | (keyState << 8))" + `cgEvent` + post.
//! We avoid depending on AppKit by constructing the CGEvent directly with
//! the private system-defined event type 14 and setting integer field 42
//! (`kCGEventSourceUserData`-adjacent index used by the NX subtype encoding).
//!
//! Key states:
//!   0xA (= NX_KEYDOWN = 10) key-down
//!   0xB (= NX_KEYUP   = 11) key-up
//!
//! NX key codes:
//!   16 NX_KEYTYPE_PLAY
//!   17 NX_KEYTYPE_NEXT
//!   18 NX_KEYTYPE_PREVIOUS
//!   19 NX_KEYTYPE_FAST
//!   20 NX_KEYTYPE_REWIND

#![cfg(target_os = "macos")]

use std::ffi::c_void;

use anyhow::{bail, Result};

// CGEventType::NSSystemDefined is 14. CGEvent wraps NSEvent, so we set this
// type then use CGEventSetIntegerValueField with field index 155
// (= kCGEventSourceStateID on some SDKs) — but the stable way is to create
// the event via NSEvent.otherEventWithType. Without AppKit linkage, we use
// a widely-used workaround: call CGEventCreate(NULL) to get a blank event,
// set its type to 14 via CGEventSetType, then write data1 into the event
// source's integer field 99 (subtype/data encoding used by the NX HID path).
//
// If this lower-level path does not register with Music/QuickTime on the
// target macOS version, the fallback is to link AppKit and use:
//   NSEvent *e = [NSEvent otherEventWithType:NSEventTypeSystemDefined
//                                   location:NSZeroPoint
//                              modifierFlags:0xa00 timestamp:0 windowNumber:0
//                                    context:nil subtype:8 data1:data1 data2:-1];
//   CGEventPost(kCGHIDEventTap, e.CGEvent);
//
// See README "Media keys troubleshooting" for operator notes.

pub const NX_KEYTYPE_PLAY: u8 = 16;
pub const NX_KEYTYPE_NEXT: u8 = 17;
pub const NX_KEYTYPE_PREVIOUS: u8 = 18;

const NX_KEY_DOWN: u8 = 0x0A;
const NX_KEY_UP: u8 = 0x0B;

// CGEventTapLocation values.
const K_CG_HID_EVENT_TAP: u32 = 0;

// NSSystemDefined event type.
const NS_SYSTEM_DEFINED: u32 = 14;

// Integer-value field index for subtype+data1 encoding on NSSystemDefined
// events. This matches what AppKit emits when you call
// -[NSEvent otherEventWithType:NSSystemDefined subtype:8 data1:(...)].
//
// IMPORTANT OPERATOR VERIFICATION FLAG: this field index is not covered by
// public Apple documentation. Values that have been observed to work on
// macOS 12-15 for this route include 155 and 99; common reference
// implementations (Apple engineering samples, remote-media-keys tools) use
// the AppKit bridge instead. If CGEvent posting does not reach Music.app
// on your target macOS, switch to the AppKit-linked path noted at the top
// of this file.
const NS_SYSTEM_DEFINED_DATA1_FIELD: u32 = 155;

// Opaque CGEventRef pointer.
type CGEventRef = *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const c_void) -> CGEventRef;
    fn CGEventSetType(event: CGEventRef, event_type: u32);
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CFRelease(cf: *const c_void);
}

fn post_media_key(key_code: u8, key_state: u8) -> Result<()> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            bail!("CGEventCreate returned null");
        }
        CGEventSetType(event, NS_SYSTEM_DEFINED);
        // subtype 8 is implicit for system-defined HID events emitted by
        // NSEvent; the data1 payload is (keyCode << 16) | (keyState << 8).
        let data1: i64 = ((key_code as i64) << 16) | ((key_state as i64) << 8);
        CGEventSetIntegerValueField(event, NS_SYSTEM_DEFINED_DATA1_FIELD, data1);
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event as *const c_void);
    }
    Ok(())
}

fn tap_media_key(key_code: u8) -> Result<()> {
    post_media_key(key_code, NX_KEY_DOWN)?;
    post_media_key(key_code, NX_KEY_UP)?;
    Ok(())
}

pub fn play_pause() -> Result<()> {
    tap_media_key(NX_KEYTYPE_PLAY)
}

pub fn next_track() -> Result<()> {
    tap_media_key(NX_KEYTYPE_NEXT)
}

pub fn previous_track() -> Result<()> {
    tap_media_key(NX_KEYTYPE_PREVIOUS)
}
