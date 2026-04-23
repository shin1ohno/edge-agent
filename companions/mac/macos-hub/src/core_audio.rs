//! Core Audio HAL FFI wrapper.
//!
//! Covers three responsibilities:
//!   1. Enumerate output devices (`list_outputs`)
//!   2. Get / set the default output device (`get_default_output`, `set_default_output`)
//!   3. Get / set the system-wide volume
//!      (`get_system_volume`, `set_system_volume`)
//!
//! The system-volume call uses the "hardware service" layer
//! (`AudioHardwareServiceGetPropertyData` + `kAudioHardwareServiceDeviceProperty_VirtualMainVolume`)
//! rather than the lower-level per-device selector. The hardware-service
//! layer routes to the correct underlying device for us and mirrors what
//! AppleScript / osascript / the menu bar do.
//!
//! All `AudioObjectID` / `AudioDeviceID` are `u32` in Core Audio HAL.

#![cfg(target_os = "macos")]

use std::mem::{size_of, zeroed, MaybeUninit};

use anyhow::{anyhow, Context};
use core_audio_sys::*;
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringGetCString, CFStringGetLength,
    CFStringGetMaximumSizeForEncoding, CFStringRef,
};

#[derive(Debug, Clone)]
pub struct OutputDevice {
    pub id: AudioDeviceID,
    pub uid: String,
    pub name: String,
    pub transport_type: u32,
    pub is_airplay: bool,
}

// --- low-level helpers ----------------------------------------------------

fn cfstring_to_string(s: CFStringRef) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let len = CFStringGetLength(s);
        let max = CFStringGetMaximumSizeForEncoding(len, kCFStringEncodingUTF8) + 1;
        let mut buf: Vec<u8> = vec![0; max as usize];
        if CFStringGetCString(
            s,
            buf.as_mut_ptr() as *mut i8,
            max,
            kCFStringEncodingUTF8,
        ) == 0
        {
            return String::new();
        }
        if let Some(nul) = buf.iter().position(|&b| b == 0) {
            buf.truncate(nul);
        }
        String::from_utf8(buf).unwrap_or_default()
    }
}

/// Fetch a fixed-size POD property (u32, AudioDeviceID, etc.) from a Core
/// Audio object.
unsafe fn get_property<T>(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> anyhow::Result<T> {
    let mut out = MaybeUninit::<T>::uninit();
    let mut size = size_of::<T>() as u32;
    let status = AudioObjectGetPropertyData(
        object_id,
        address,
        0,
        std::ptr::null(),
        &mut size,
        out.as_mut_ptr() as *mut _,
    );
    if status != 0 {
        return Err(anyhow!(
            "AudioObjectGetPropertyData failed (status={}, selector=0x{:x})",
            status,
            address.mSelector
        ));
    }
    Ok(out.assume_init())
}

unsafe fn set_property<T>(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
    value: &T,
) -> anyhow::Result<()> {
    let status = AudioObjectSetPropertyData(
        object_id,
        address,
        0,
        std::ptr::null(),
        size_of::<T>() as u32,
        value as *const T as *const _,
    );
    if status != 0 {
        return Err(anyhow!(
            "AudioObjectSetPropertyData failed (status={}, selector=0x{:x})",
            status,
            address.mSelector
        ));
    }
    Ok(())
}

unsafe fn get_property_size(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> anyhow::Result<u32> {
    let mut size: u32 = 0;
    let status = AudioObjectGetPropertyDataSize(
        object_id,
        address,
        0,
        std::ptr::null(),
        &mut size,
    );
    if status != 0 {
        return Err(anyhow!(
            "AudioObjectGetPropertyDataSize failed (status={}, selector=0x{:x})",
            status,
            address.mSelector
        ));
    }
    Ok(size)
}

// --- public API -----------------------------------------------------------

/// Enumerate all audio objects that have at least one output stream.
pub fn list_outputs() -> anyhow::Result<Vec<OutputDevice>> {
    unsafe {
        let devices_addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };

        let size = get_property_size(kAudioObjectSystemObject, &devices_addr)?;
        let count = (size as usize) / size_of::<AudioDeviceID>();
        let mut ids: Vec<AudioDeviceID> = vec![0; count];
        let mut size_inout = size;
        let status = AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &devices_addr,
            0,
            std::ptr::null(),
            &mut size_inout,
            ids.as_mut_ptr() as *mut _,
        );
        if status != 0 {
            return Err(anyhow!(
                "AudioObjectGetPropertyData(Devices) failed (status={})",
                status
            ));
        }

        let mut outputs = Vec::new();
        for &id in &ids {
            if !has_output_streams(id)? {
                continue;
            }
            match describe_device(id) {
                Ok(dev) => outputs.push(dev),
                Err(e) => {
                    tracing::warn!("Skipping device {}: {}", id, e);
                }
            }
        }
        Ok(outputs)
    }
}

unsafe fn has_output_streams(device_id: AudioDeviceID) -> anyhow::Result<bool> {
    let streams_addr = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyStreams,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: kAudioObjectPropertyElementMain,
    };
    let size = get_property_size(device_id, &streams_addr).unwrap_or(0);
    Ok(size > 0)
}

unsafe fn describe_device(id: AudioDeviceID) -> anyhow::Result<OutputDevice> {
    let uid = read_cfstring_property(
        id,
        kAudioDevicePropertyDeviceUID,
        kAudioObjectPropertyScopeGlobal,
    )
    .context("reading device UID")?;
    let name = read_cfstring_property(
        id,
        kAudioObjectPropertyName,
        kAudioObjectPropertyScopeGlobal,
    )
    .context("reading device name")?;

    let transport_addr = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyTransportType,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let transport_type: u32 =
        get_property(id, &transport_addr).unwrap_or(0);
    // kAudioDeviceTransportTypeAirPlay = 'airp' = 0x61697270
    let is_airplay = transport_type == 0x6169_7270;

    Ok(OutputDevice {
        id,
        uid,
        name,
        transport_type,
        is_airplay,
    })
}

unsafe fn read_cfstring_property(
    device_id: AudioDeviceID,
    selector: u32,
    scope: u32,
) -> anyhow::Result<String> {
    let addr = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut cfstr: CFStringRef = zeroed();
    let mut size = size_of::<CFStringRef>() as u32;
    let status = AudioObjectGetPropertyData(
        device_id,
        &addr,
        0,
        std::ptr::null(),
        &mut size,
        &mut cfstr as *mut _ as *mut _,
    );
    if status != 0 {
        return Err(anyhow!(
            "AudioObjectGetPropertyData(CFString selector=0x{:x}) failed (status={})",
            selector,
            status
        ));
    }
    let s = cfstring_to_string(cfstr);
    if !cfstr.is_null() {
        CFRelease(cfstr as CFTypeRef);
    }
    Ok(s)
}

/// Get the current default output device's AudioDeviceID.
pub fn get_default_output() -> anyhow::Result<AudioDeviceID> {
    unsafe {
        let addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        get_property::<AudioDeviceID>(kAudioObjectSystemObject, &addr)
    }
}

/// Set the default output device by AudioDeviceID.
pub fn set_default_output(id: AudioDeviceID) -> anyhow::Result<()> {
    unsafe {
        let addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        set_property(kAudioObjectSystemObject, &addr, &id)
    }
}

// --- volume ---------------------------------------------------------------
//
// Volume uses the "hardware service" layer + VirtualMainVolume selector,
// which routes through the currently-active output device.
//
// kAudioHardwareServiceDeviceProperty_VirtualMainVolume = 'vmvc'
const K_VIRTUAL_MAIN_VOLUME: u32 = 0x766d_7663; // 'vmvc'

// core-audio-sys 0.2 does not re-export the AudioHardwareService* symbols in
// all releases, so declare them explicitly. They live in CoreAudio.framework
// and have the same signatures as AudioObject*.
extern "C" {
    fn AudioHardwareServiceGetPropertyData(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const std::ffi::c_void,
        ioDataSize: *mut u32,
        outData: *mut std::ffi::c_void,
    ) -> i32;

    fn AudioHardwareServiceSetPropertyData(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const std::ffi::c_void,
        inDataSize: u32,
        inData: *const std::ffi::c_void,
    ) -> i32;
}

/// Get the system volume of the current default output (0.0 - 1.0).
pub fn get_system_volume() -> anyhow::Result<f32> {
    let device = get_default_output()?;
    unsafe {
        let addr = AudioObjectPropertyAddress {
            mSelector: K_VIRTUAL_MAIN_VOLUME,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut value: f32 = 0.0;
        let mut size = size_of::<f32>() as u32;
        let status = AudioHardwareServiceGetPropertyData(
            device,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut value as *mut _ as *mut _,
        );
        if status != 0 {
            return Err(anyhow!(
                "AudioHardwareServiceGetPropertyData(VirtualMainVolume) failed (status={})",
                status
            ));
        }
        Ok(value.clamp(0.0, 1.0))
    }
}

/// Set the system volume of the current default output (0.0 - 1.0).
pub fn set_system_volume(level: f32) -> anyhow::Result<()> {
    let device = get_default_output()?;
    let clamped = level.clamp(0.0, 1.0);
    unsafe {
        let addr = AudioObjectPropertyAddress {
            mSelector: K_VIRTUAL_MAIN_VOLUME,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: kAudioObjectPropertyElementMain,
        };
        let status = AudioHardwareServiceSetPropertyData(
            device,
            &addr,
            0,
            std::ptr::null(),
            size_of::<f32>() as u32,
            &clamped as *const _ as *const _,
        );
        if status != 0 {
            return Err(anyhow!(
                "AudioHardwareServiceSetPropertyData(VirtualMainVolume) failed (status={})",
                status
            ));
        }
        Ok(())
    }
}

/// Convenience: default output as `OutputDevice`.
pub fn get_default_output_device() -> anyhow::Result<OutputDevice> {
    let id = get_default_output()?;
    unsafe { describe_device(id) }
}

/// Look up an output device by its persistent UID. Fresh enumeration each
/// call — AudioDeviceIDs are ephemeral and not safe to cache across
/// enumerations. UID is the stable identifier.
pub fn find_output_by_uid(uid: &str) -> anyhow::Result<OutputDevice> {
    let outputs = list_outputs()?;
    outputs
        .into_iter()
        .find(|d| d.uid == uid)
        .ok_or_else(|| anyhow!("no output device with uid '{}'", uid))
}
