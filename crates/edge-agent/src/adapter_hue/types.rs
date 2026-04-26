//! Hue v2 JSON shapes. Only the fields we actually use are modeled; anything
//! else is ignored via serde so the adapter survives bridge firmware updates.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct LightsResponse {
    #[serde(default)]
    pub errors: Vec<HueError>,
    pub data: Vec<Light>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HueError {
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Light {
    pub id: String,
    #[serde(default)]
    pub metadata: LightMetadata,
    #[serde(default)]
    pub on: OnState,
    #[serde(default)]
    pub dimming: Option<Dimming>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LightMetadata {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct OnState {
    pub on: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Dimming {
    pub brightness: f64,
}

/// Payload for `PUT /clip/v2/resource/light/{id}`. Fields are all optional so
/// the caller sends only what it wants to change.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LightUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<OnState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimming: Option<Dimming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimming_delta: Option<DimmingDelta>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DimmingDelta {
    pub action: DimmingAction,
    pub brightness_delta: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DimmingAction {
    Up,
    Down,
}

/// Inbound SSE event frame. Only the "update" type carries state we care
/// about; others are ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct SseEvent {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub data: Vec<SseEventData>,
}

/// One inner record inside an SSE update. The bridge multiplexes every
/// resource type through this shape, so each variant's fields are
/// `Option<_>` and only populated for matching `resource_type`s.
#[derive(Debug, Clone, Deserialize)]
pub struct SseEventData {
    pub id: String,
    #[serde(default, rename = "type")]
    pub resource_type: String,
    // `light` arms:
    #[serde(default)]
    pub on: Option<OnState>,
    #[serde(default)]
    pub dimming: Option<Dimming>,
    // `button` arm:
    #[serde(default)]
    pub button: Option<ButtonEvent>,
    // `relative_rotary` arm:
    #[serde(default)]
    pub relative_rotary: Option<RelativeRotaryEvent>,
    // `device_power` arm:
    #[serde(default)]
    pub power_state: Option<PowerState>,
    // Common to button / relative_rotary / device_power events: points at
    // the parent `device` resource so the consumer can group by physical
    // controller without re-querying the bridge.
    #[serde(default)]
    pub owner: Option<ResourceRef>,
    // Only present on `button` events; carries control_id (1..=4 for the
    // Tap Dial). Lights also include metadata, but with `name` instead.
    #[serde(default)]
    pub metadata: Option<ResourceMeta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ButtonEvent {
    #[serde(default)]
    pub button_report: Option<ButtonReport>,
    /// Older firmwares (Hue v2 < 1.50) stream `last_event` (string) instead
    /// of the structured `button_report`. Capture both.
    #[serde(default)]
    pub last_event: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ButtonReport {
    /// `"initial_press" | "repeat" | "short_release" | "long_press" | "long_release"`
    pub event: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelativeRotaryEvent {
    #[serde(default)]
    pub last_event: Option<RotaryLastEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RotaryLastEvent {
    /// `"start" | "repeat"`
    pub action: String,
    pub rotation: RotaryRotation,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RotaryRotation {
    /// `"clock_wise" | "counter_clock_wise"`
    pub direction: String,
    pub steps: i32,
    #[serde(default)]
    pub duration: Option<i32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PowerState {
    pub battery_level: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceRef {
    pub rid: String,
    #[serde(default)]
    pub rtype: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceMeta {
    #[serde(default)]
    pub control_id: Option<u8>,
    #[serde(default)]
    pub name: Option<String>,
}

// Top-level GET /clip/v2/resource/device response shape. Only fields
// needed for Tap Dial enumeration are modelled.
#[derive(Debug, Clone, Deserialize)]
pub struct DevicesResponse {
    #[serde(default)]
    pub data: Vec<HueDevice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HueDevice {
    pub id: String,
    #[serde(default)]
    pub product_data: Option<ProductData>,
    #[serde(default)]
    pub metadata: Option<DeviceMetadata>,
    #[serde(default)]
    pub services: Vec<ResourceRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductData {
    #[serde(default)]
    pub product_name: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeviceMetadata {
    #[serde(default)]
    pub name: Option<String>,
}

// Top-level GET /clip/v2/resource/button response (and similar for
// relative_rotary, device_power). Same envelope, different payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ButtonsResponse {
    #[serde(default)]
    pub data: Vec<HueButton>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HueButton {
    pub id: String,
    #[serde(default)]
    pub owner: Option<ResourceRef>,
    #[serde(default)]
    pub metadata: Option<ResourceMeta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DevicePowerResponse {
    #[serde(default)]
    pub data: Vec<HueDevicePower>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HueDevicePower {
    pub id: String,
    #[serde(default)]
    pub owner: Option<ResourceRef>,
    #[serde(default)]
    pub power_state: Option<PowerState>,
}
