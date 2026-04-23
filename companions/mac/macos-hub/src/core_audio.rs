//! Core Audio HAL FFI wrapper.
//!
//! Mirrors `/tmp/core-audio-spike-ref.swift`, which was verified on macOS 15
//! Apple Silicon. See the spike file for original selector / scope choices.
//!
//! Property selectors and scopes used:
//! - `kAudioHardwarePropertyDevices` (global scope) — system object → all device IDs
//! - `kAudioDevicePropertyStreams`   (output scope) — filter for output devices
//! - `kAudioObjectPropertyName`      (global scope) — device name (CFString)
//! - `kAudioDevicePropertyDeviceUID` (global scope) — stable UID (CFString)
//! - `kAudioDevicePropertyTransportType` (output scope) — FourCC, `airp` = AirPlay
//! - `kAudioHardwarePropertyDefaultOutputDevice` (global scope) — get/set default
//! - `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` (output scope,
//!   element main) — system volume 0.0..=1.0 via `AudioHardwareServiceGetPropertyData`
//!
//! We implement raw FFI rather than pulling `coreaudio-sys` (RustAudio). The
//! surface is narrow: 4 AudioObject property functions + 2 HardwareService
//! property functions. All selector constants are declared as FourCC literals
//! below so they are audit-able against Apple's headers.

#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::ptr;

use anyhow::{anyhow, bail, Result};
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringGetCString, CFStringGetLength, CFStringRef,
};
use serde::Serialize;

// -------------------- Core Audio types --------------------

pub type AudioObjectID = u32;
pub type AudioDeviceID = u32;
pub type AudioObjectPropertySelector = u32;
pub type AudioObjectPropertyScope = u32;
pub type AudioObjectPropertyElement = u32;
pub type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioObjectPropertyAddress {
    pub mSelector: AudioObjectPropertySelector,
    pub mScope: AudioObjectPropertyScope,
    pub mElement: AudioObjectPropertyElement,
}

// -------------------- FourCC constants --------------------

/// Compile-time helper: pack 4 ASCII chars into a big-endian UInt32 OSType.
const fn four_cc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
}

pub const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;

// Selectors. FourCC letters from <CoreAudio/AudioHardware.h>.
pub const K_AUDIO_HARDWARE_PROPERTY_DEVICES: AudioObjectPropertySelector = four_cc(b"dev#");
pub const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector =
    four_cc(b"dOut");
pub const K_AUDIO_DEVICE_PROPERTY_STREAMS: AudioObjectPropertySelector = four_cc(b"stm#");
pub const K_AUDIO_DEVICE_PROPERTY_DEVICE_UID: AudioObjectPropertySelector = four_cc(b"uid ");
pub const K_AUDIO_OBJECT_PROPERTY_NAME: AudioObjectPropertySelector = four_cc(b"lnam");
pub const K_AUDIO_DEVICE_PROPERTY_TRANSPORT_TYPE: AudioObjectPropertySelector = four_cc(b"tran");
pub const K_AUDIO_HW_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME: AudioObjectPropertySelector =
    four_cc(b"vmvc");

// Scopes.
pub const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope = four_cc(b"glob");
pub const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: AudioObjectPropertyScope = four_cc(b"outp");
pub const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;

// Transport type: 'airp' = AirPlay.
pub const TRANSPORT_TYPE_AIRPLAY: u32 = four_cc(b"airp");

// -------------------- extern "C" declarations --------------------
//
// OPERATOR VERIFICATION FLAG: these signatures are hand-written from
// <CoreAudio/AudioHardware.h> and <CoreAudio/AudioHardwareService.h>. They
// match the published Apple headers as of macOS 15 SDK. If a future SDK
// changes any signature (unlikely — these are C ABI), link errors will
// surface at build time on macOS. CoreFoundation + CoreAudio frameworks
// are linked via #[link(name = "Foo", kind = "framework")] below.

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        out_data_size: *mut u32,
    ) -> OSStatus;

    fn AudioObjectGetPropertyData(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> OSStatus;

    fn AudioObjectSetPropertyData(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        in_data_size: u32,
        in_data: *const c_void,
    ) -> OSStatus;

    // AudioHardwareService* — in the CoreAudio framework since macOS 10.5.
    fn AudioHardwareServiceGetPropertyData(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> OSStatus;

    fn AudioHardwareServiceSetPropertyData(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        in_data_size: u32,
        in_data: *const c_void,
    ) -> OSStatus;
}

// -------------------- public model --------------------

#[derive(Debug, Clone, Serialize)]
pub struct OutputDevice {
    pub id: u32,
    pub uid: String,
    pub name: String,
    pub transport_type: u32,
    pub is_airplay: bool,
}

// -------------------- helpers --------------------

fn prop_addr(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

fn check(status: OSStatus, what: &str) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        bail!("{}: OSStatus={}", what, status)
    }
}

/// Read a CFString-typed property and copy it into an owned `String`.
/// Caller assumes a +1 retain count on the returned CFString and releases.
unsafe fn get_cf_string_property(
    device: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> Result<String> {
    let addr = prop_addr(selector, scope);
    let mut size: u32 = std::mem::size_of::<CFStringRef>() as u32;
    let mut value: CFStringRef = ptr::null();

    let status = AudioObjectGetPropertyData(
        device,
        &addr,
        0,
        ptr::null(),
        &mut size,
        &mut value as *mut CFStringRef as *mut c_void,
    );
    check(status, &format!("get CFString selector={:x}", selector))?;

    if value.is_null() {
        return Ok(String::new());
    }

    let len = CFStringGetLength(value);
    // Worst-case UTF-8 byte count is 4 * UTF-16 code-unit count + 1 NUL.
    let capacity = (len * 4 + 1) as usize;
    let mut buf = vec![0u8; capacity];
    let ok = CFStringGetCString(
        value,
        buf.as_mut_ptr() as *mut i8,
        capacity as isize,
        kCFStringEncodingUTF8,
    );

    CFRelease(value as CFTypeRef);

    if ok == 0 {
        bail!("CFStringGetCString failed");
    }

    let nul = buf.iter().position(|&b| b == 0).unwrap_or(capacity);
    buf.truncate(nul);
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

unsafe fn get_u32_property(
    device: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> Result<u32> {
    let addr = prop_addr(selector, scope);
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let mut value: u32 = 0;
    let status = AudioObjectGetPropertyData(
        device,
        &addr,
        0,
        ptr::null(),
        &mut size,
        &mut value as *mut u32 as *mut c_void,
    );
    check(status, &format!("get u32 selector={:x}", selector))?;
    Ok(value)
}

unsafe fn has_output_streams(device: AudioObjectID) -> bool {
    let addr = prop_addr(
        K_AUDIO_DEVICE_PROPERTY_STREAMS,
        K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
    );
    let mut size: u32 = 0;
    let status = AudioObjectGetPropertyDataSize(device, &addr, 0, ptr::null(), &mut size);
    status == 0 && size > 0
}

// -------------------- public API --------------------

/// Enumerate every output-capable audio device.
pub fn list_outputs() -> Result<Vec<OutputDevice>> {
    unsafe {
        let addr = prop_addr(
            K_AUDIO_HARDWARE_PROPERTY_DEVICES,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        );
        let mut size: u32 = 0;
        check(
            AudioObjectGetPropertyDataSize(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                ptr::null(),
                &mut size,
            ),
            "devices size",
        )?;
        let count = size as usize / std::mem::size_of::<AudioDeviceID>();
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut ids = vec![0 as AudioDeviceID; count];
        check(
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                ptr::null(),
                &mut size,
                ids.as_mut_ptr() as *mut c_void,
            ),
            "devices data",
        )?;

        let mut out = Vec::new();
        for id in ids {
            if !has_output_streams(id) {
                continue;
            }
            let uid = get_cf_string_property(
                id,
                K_AUDIO_DEVICE_PROPERTY_DEVICE_UID,
                K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            )
            .unwrap_or_default();
            let name = get_cf_string_property(
                id,
                K_AUDIO_OBJECT_PROPERTY_NAME,
                K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            )
            .unwrap_or_default();
            // Transport may be absent on aggregate devices; treat absent as 0.
            let transport = get_u32_property(
                id,
                K_AUDIO_DEVICE_PROPERTY_TRANSPORT_TYPE,
                K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
            )
            .unwrap_or(0);
            out.push(OutputDevice {
                id,
                uid,
                name,
                transport_type: transport,
                is_airplay: transport == TRANSPORT_TYPE_AIRPLAY,
            });
        }
        Ok(out)
    }
}

/// Read `kAudioHardwarePropertyDefaultOutputDevice`.
pub fn get_default_output() -> Result<u32> {
    unsafe {
        let addr = prop_addr(
            K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        );
        let mut size: u32 = std::mem::size_of::<AudioDeviceID>() as u32;
        let mut id: AudioDeviceID = 0;
        check(
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                ptr::null(),
                &mut size,
                &mut id as *mut AudioDeviceID as *mut c_void,
            ),
            "get default output",
        )?;
        Ok(id)
    }
}

/// Write `kAudioHardwarePropertyDefaultOutputDevice`.
pub fn set_default_output(id: u32) -> Result<()> {
    unsafe {
        let addr = prop_addr(
            K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        );
        let size = std::mem::size_of::<AudioDeviceID>() as u32;
        let id_copy: AudioDeviceID = id;
        check(
            AudioObjectSetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                ptr::null(),
                size,
                &id_copy as *const AudioDeviceID as *const c_void,
            ),
            "set default output",
        )
    }
}

/// Read system volume (0.0..=1.0) of the current default output device via
/// `AudioHardwareServiceGetPropertyData` — the selector that controls the
/// menubar slider.
pub fn get_system_volume() -> Result<f32> {
    unsafe {
        let device = get_default_output()?;
        if device == 0 {
            bail!("no default output device");
        }
        let addr = prop_addr(
            K_AUDIO_HW_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
        );
        let mut size: u32 = std::mem::size_of::<f32>() as u32;
        let mut value: f32 = 0.0;
        check(
            AudioHardwareServiceGetPropertyData(
                device,
                &addr,
                0,
                ptr::null(),
                &mut size,
                &mut value as *mut f32 as *mut c_void,
            ),
            "get virtual main volume",
        )?;
        Ok(value.clamp(0.0, 1.0))
    }
}

/// Set system volume (0.0..=1.0). Out-of-range inputs are clamped.
pub fn set_system_volume(level: f32) -> Result<()> {
    unsafe {
        let device = get_default_output()?;
        if device == 0 {
            bail!("no default output device");
        }
        let clamped = level.clamp(0.0, 1.0);
        let addr = prop_addr(
            K_AUDIO_HW_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
            K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
        );
        let size = std::mem::size_of::<f32>() as u32;
        check(
            AudioHardwareServiceSetPropertyData(
                device,
                &addr,
                0,
                ptr::null(),
                size,
                &clamped as *const f32 as *const c_void,
            ),
            "set virtual main volume",
        )
    }
}

/// Convenience: look up an `OutputDevice` by UID via a fresh enumeration.
pub fn find_device_by_uid(uid: &str) -> Result<OutputDevice> {
    list_outputs()?
        .into_iter()
        .find(|d| d.uid == uid)
        .ok_or_else(|| anyhow!("no output device with uid={}", uid))
}
