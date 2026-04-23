//! Media-key injection via AppKit's NSEvent.
//!
//! Media keys on macOS are `NSEventType::SystemDefined` (14) events with
//! subtype 8 and a data1 payload encoding `(keyCode << 16) | (keyState << 8)`.
//! The previous CGEvent-only path (CGEventCreate + CGEventSetType(14) +
//! CGEventSetIntegerValueField(..., 155, data1)) did NOT reach Music.app on
//! macOS 15 — the integer field index is not part of public API and the
//! NSEvent wrapper is the canonical bridge.
//!
//! This implementation constructs the NSEvent via
//! `+[NSEvent otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:]`,
//! extracts its CGEvent ref, and posts via `CGEventPost(kCGHIDEventTap, ...)`.
//!
//! `CGEventPost` silently drops events if the process lacks Accessibility
//! permission (System Settings → Privacy & Security → Accessibility →
//! enable the app / terminal from which macos-hub was launched). An
//! explicit `AXIsProcessTrusted()` check at startup is provided.
//!
//! Key codes (from IOKit/hidsystem/ev_keymap.h):
//!   16 NX_KEYTYPE_PLAY
//!   17 NX_KEYTYPE_NEXT
//!   18 NX_KEYTYPE_PREVIOUS
//!   19 NX_KEYTYPE_FAST
//!   20 NX_KEYTYPE_REWIND
//!
//! Key states:
//!   0xA (= 10) NX key-down
//!   0xB (= 11) NX key-up

#![cfg(target_os = "macos")]

use std::ffi::c_void;

use anyhow::{bail, Result};
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::NSPoint;

pub const NX_KEYTYPE_PLAY: u8 = 16;
pub const NX_KEYTYPE_NEXT: u8 = 17;
pub const NX_KEYTYPE_PREVIOUS: u8 = 18;

const NX_KEY_DOWN: u8 = 0x0A;
const NX_KEY_UP: u8 = 0x0B;

// NSEventType::SystemDefined
const NS_SYSTEM_DEFINED: u64 = 14;
// NSSystemDefined subtype for media/HID keys.
const MEDIA_KEY_SUBTYPE: i16 = 8;

// CGEventTapLocation::kCGHIDEventTap
const K_CG_HID_EVENT_TAP: u32 = 0;

type CGEventRef = *mut c_void;

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

/// Returns true if the process has Accessibility permission.
/// `CGEventPost` silently no-ops without it — calling this at startup lets
/// us warn loudly instead of puzzling over why play/pause does nothing.
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

fn post_media_key(key_code: u8, key_state: u8) -> Result<()> {
    let data1: isize = ((key_code as isize) << 16) | ((key_state as isize) << 8);

    autoreleasepool(|_| unsafe {
        let cls = class!(NSEvent);
        let event: *mut AnyObject = msg_send![
            cls,
            otherEventWithType: NS_SYSTEM_DEFINED,
            location: NSPoint::new(0.0, 0.0),
            modifierFlags: 0xa00u64,
            timestamp: 0f64,
            windowNumber: 0isize,
            context: std::ptr::null::<AnyObject>(),
            subtype: MEDIA_KEY_SUBTYPE,
            data1: data1,
            data2: -1isize,
        ];
        if event.is_null() {
            bail!("NSEvent otherEventWithType: returned nil");
        }
        let cg_event: CGEventRef = msg_send![event, CGEvent];
        if cg_event.is_null() {
            bail!("NSEvent.CGEvent returned null");
        }
        CGEventPost(K_CG_HID_EVENT_TAP, cg_event);
        Ok(())
    })
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
