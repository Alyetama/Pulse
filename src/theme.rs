//! Colour palette, usage-threshold colouring, fonts and global egui style.

use egui::{Color32, CornerRadius, Margin};
use std::sync::Arc;

/// Colours resolved for the current light/dark appearance.
pub struct Palette {
    pub bg: Color32,
    pub card: Color32,
    pub card_stroke: Color32,
    pub text: Color32,
    pub subtle: Color32,
    pub faint: Color32,
    pub separator: Color32,
    pub accent: Color32,
    pub track: Color32,
}

impl Palette {
    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Palette {
                bg: Color32::from_rgba_unmultiplied(30, 30, 34, 244),
                card: Color32::from_rgba_unmultiplied(255, 255, 255, 12),
                card_stroke: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                text: Color32::from_rgb(245, 245, 247),
                subtle: Color32::from_rgb(160, 160, 168),
                faint: Color32::from_rgb(120, 120, 128),
                separator: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
                accent: Color32::from_rgb(94, 140, 255),
                track: Color32::from_rgba_unmultiplied(255, 255, 255, 24),
            }
        } else {
            Palette {
                bg: Color32::from_rgba_unmultiplied(247, 247, 249, 248),
                card: Color32::from_rgba_unmultiplied(0, 0, 0, 10),
                card_stroke: Color32::from_rgba_unmultiplied(0, 0, 0, 16),
                text: Color32::from_rgb(28, 28, 30),
                subtle: Color32::from_rgb(110, 110, 116),
                faint: Color32::from_rgb(150, 150, 156),
                separator: Color32::from_rgba_unmultiplied(0, 0, 0, 20),
                accent: Color32::from_rgb(44, 107, 255),
                track: Color32::from_rgba_unmultiplied(0, 0, 0, 22),
            }
        }
    }
}

/// Apple-ish system colours used for the green→amber→red usage ramp.
const GREEN: (u8, u8, u8) = (52, 199, 89);
const AMBER: (u8, u8, u8) = (255, 159, 10);
const RED: (u8, u8, u8) = (255, 69, 58);

fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// Colour for a 0..1 usage fraction: green when idle, amber mid, red when hot.
pub fn usage_color(frac: f32) -> Color32 {
    let t = frac.clamp(0.0, 1.0);
    if t <= 0.65 {
        lerp(GREEN, AMBER, t / 0.65)
    } else {
        lerp(AMBER, RED, (t - 0.65) / 0.35)
    }
}

/// A slightly desaturated variant of the usage colour, for sparkline fills.
pub fn usage_fill(frac: f32) -> Color32 {
    let c = usage_color(frac);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 46)
}

/// Try to load the macOS system font so the panel feels native. Falls back to
/// egui's bundled font if San Francisco cannot be read.
pub fn install_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/SFNSDisplay.ttf",
        "/System/Library/Fonts/SFNSText.ttf",
        "/Library/Fonts/SF-Pro.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("system".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
            if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                list.insert(0, "system".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}

/// Global spacing / rounding tweaks applied once at startup.
pub fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.window_margin = Margin::same(0);
        style.visuals.clip_rect_margin = 0.0;
    });
}

pub const CARD_RADIUS: CornerRadius = CornerRadius::same(12);
pub const PANEL_RADIUS: CornerRadius = CornerRadius::same(16);
