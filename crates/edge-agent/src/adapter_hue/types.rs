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

#[derive(Debug, Clone, Deserialize)]
pub struct SseEventData {
    pub id: String,
    #[serde(default, rename = "type")]
    pub resource_type: String,
    #[serde(default)]
    pub on: Option<OnState>,
    #[serde(default)]
    pub dimming: Option<Dimming>,
}
