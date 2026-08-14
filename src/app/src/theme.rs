use autofarm_sim::CropInstance;
use bevy::prelude::*;

pub const BACKGROUND: Color = Color::srgb(0.035, 0.055, 0.055);
pub const PANEL: Color = Color::srgba(0.035, 0.08, 0.075, 0.96);
pub const PANEL_ALT: Color = Color::srgba(0.07, 0.12, 0.10, 0.94);
pub const PANEL_BORDER: Color = Color::srgb(0.36, 0.56, 0.40);
pub const TEXT: Color = Color::srgb(0.91, 0.91, 0.77);
pub const MUTED: Color = Color::srgb(0.56, 0.68, 0.59);
pub const ACCENT: Color = Color::srgb(0.36, 0.90, 0.72);
pub const GOLD: Color = Color::srgb(0.94, 0.70, 0.25);
pub const DANGER: Color = Color::srgb(0.92, 0.30, 0.26);
pub const BUTTON: Color = Color::srgb(0.10, 0.24, 0.19);
pub const BUTTON_HOVER: Color = Color::srgb(0.15, 0.36, 0.27);
pub const BUTTON_PRESS: Color = Color::srgb(0.30, 0.64, 0.40);

#[must_use]
pub fn crop_color(crop: &CropInstance) -> Color {
    let health_scale = f32::from(crop.health) / 100.0;
    let base = match crop.crop_id.as_str() {
        "rice" if crop.stage_index >= 4 => [0.98, 0.72, 0.12],
        "rice" => [0.42, 0.88, 0.22],
        "wheat" => [0.93, 0.69, 0.18],
        "potato" => [0.45, 0.65, 0.22],
        "tomato" => [0.90, 0.20, 0.12],
        "strawberry" => [0.95, 0.24, 0.42],
        _ => [0.45, 0.75, 0.30],
    };
    let stage = 0.45 + crop.stage_index as f32 * 0.17;
    Color::srgb(
        base[0] * stage * health_scale,
        base[1] * stage * health_scale,
        base[2] * stage * health_scale,
    )
}
