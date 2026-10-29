// Hallmark · shape: app screen · genre: modern-minimal · theme: Dose (deal 1/3, seed kivo-dictation-writing-utility-8263)
// nav: top bar, wordmark left with tabs, model right · signature: the writing workbench (input, grouped actions, live result) · states: loading, empty, error, full · dose: white
// axes: light/grotesk-sans/chromatic-green · posture: committed · contrast: pass · v2.0.0
// Hallmark · pre-emit critique: P4 H4 E4 S4 R4 V4 · weakest: S, kept pharmacy-label mono for shortcuts and model IDs so the tool reads as exact, not generic
//
// Dose ported to native egui: warm paper, green-black ink, one pharmacy green
// signal, DM-Mono-style outlier via the monospace family. Both dials (white and
// night) are always installed so the OS theme switch follows Windows and macOS.

use crate::settings::Theme;
use egui::{Color32, CornerRadius, FontFamily, FontId, Response, RichText, Stroke, TextStyle, Ui};

// White dial (default).
pub const PAPER: Color32 = Color32::from_rgb(250, 247, 239);
pub const PAPER_2: Color32 = Color32::from_rgb(233, 248, 239);
pub const EDIT_BG: Color32 = Color32::from_rgb(255, 253, 248);
pub const INK: Color32 = Color32::from_rgb(12, 28, 22);
pub const RULE: Color32 = Color32::from_rgb(203, 214, 201);
pub const ACCENT: Color32 = Color32::from_rgb(0, 121, 72);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0, 95, 58);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(169, 232, 198);
pub const ACCENT_INK: Color32 = Color32::from_rgb(248, 253, 250);

// Night dial.
pub const NIGHT_PAPER: Color32 = Color32::from_rgb(15, 27, 22);
pub const NIGHT_PAPER_2: Color32 = Color32::from_rgb(22, 38, 31);
pub const NIGHT_EDIT: Color32 = Color32::from_rgb(10, 19, 15);
pub const NIGHT_INK: Color32 = Color32::from_rgb(236, 244, 239);
pub const NIGHT_RULE: Color32 = Color32::from_rgb(32, 44, 38);
pub const NIGHT_SOFT: Color32 = Color32::from_rgb(16, 60, 40);
pub const NIGHT_FOCUS: Color32 = Color32::from_rgb(123, 216, 164);
pub const NIGHT_LINK: Color32 = Color32::from_rgb(111, 211, 160);
pub const NIGHT_ERROR: Color32 = Color32::from_rgb(232, 138, 135);
pub const NIGHT_WARN: Color32 = Color32::from_rgb(232, 181, 100);

pub const ERROR: Color32 = Color32::from_rgb(190, 35, 35);
pub const SUCCESS: Color32 = Color32::from_rgb(33, 118, 60);
pub const WARN: Color32 = Color32::from_rgb(146, 84, 0);

/// Width of the centered working column. Narrow windows use the full width.
pub const MAX_CONTENT_WIDTH: f32 = 780.0;
/// App control height: 32 to 40px with room for a 44px hit target.
pub const CONTROL_HEIGHT: f32 = 36.0;

const _: () = assert!(MAX_CONTENT_WIDTH <= 800.0, "working column stays capped");

fn text_styles() -> [(TextStyle, FontId); 5] {
    [
        (
            TextStyle::Small,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(13.5, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(23.0, FontFamily::Proportional),
        ),
    ]
}

fn base_style(mut style: egui::Style) -> egui::Style {
    for (text_style, font_id) in text_styles() {
        style.text_styles.insert(text_style, font_id);
    }
    style.spacing.item_spacing = egui::vec2(12.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 20.0;
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.combo_width = 220.0;
    style
}

fn round_widgets(visuals: &mut egui::Visuals, radius: u8) {
    let corner = CornerRadius::same(radius);
    visuals.widgets.noninteractive.corner_radius = corner;
    visuals.widgets.inactive.corner_radius = corner;
    visuals.widgets.hovered.corner_radius = corner;
    visuals.widgets.active.corner_radius = corner;
    visuals.widgets.open.corner_radius = corner;
}

pub fn light_style() -> egui::Style {
    let mut style = base_style(egui::Theme::Light.default_style());
    let visuals = &mut style.visuals;
    visuals.dark_mode = false;
    visuals.panel_fill = PAPER;
    visuals.window_fill = PAPER;
    visuals.faint_bg_color = PAPER_2;
    visuals.extreme_bg_color = EDIT_BG;
    visuals.code_bg_color = PAPER_2;
    visuals.text_edit_bg_color = Some(EDIT_BG);
    visuals.warn_fg_color = WARN;
    visuals.error_fg_color = ERROR;
    visuals.hyperlink_color = ACCENT_DEEP;
    visuals.selection.bg_fill = ACCENT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, INK);
    visuals.window_stroke = Stroke::new(1.0, RULE);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.text_cursor.stroke = Stroke::new(2.0, ACCENT);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.inactive.weak_bg_fill = PAPER_2;
    visuals.widgets.hovered.weak_bg_fill = ACCENT_SOFT;
    round_widgets(visuals, 10);
    style
}

pub fn dark_style() -> egui::Style {
    let mut style = base_style(egui::Theme::Dark.default_style());
    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.panel_fill = NIGHT_PAPER;
    visuals.window_fill = NIGHT_PAPER;
    visuals.faint_bg_color = NIGHT_PAPER_2;
    visuals.extreme_bg_color = NIGHT_EDIT;
    visuals.code_bg_color = NIGHT_PAPER_2;
    visuals.text_edit_bg_color = Some(NIGHT_EDIT);
    visuals.warn_fg_color = NIGHT_WARN;
    visuals.error_fg_color = NIGHT_ERROR;
    visuals.hyperlink_color = NIGHT_LINK;
    visuals.selection.bg_fill = NIGHT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, NIGHT_INK);
    visuals.window_stroke = Stroke::new(1.0, NIGHT_RULE);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.text_cursor.stroke = Stroke::new(2.0, NIGHT_FOCUS);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, NIGHT_INK);
    visuals.widgets.inactive.weak_bg_fill = NIGHT_PAPER_2;
    visuals.widgets.hovered.weak_bg_fill = NIGHT_SOFT;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, NIGHT_INK);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, NIGHT_INK);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, NIGHT_INK);
    round_widgets(visuals, 10);
    style
}

/// Install both Dose dials and follow the user's theme preference.
/// The OS switch is picked up live because both styles stay installed and the
/// native window theme syncs from the same preference.
pub fn apply_theme(settings: &crate::settings::Settings, ctx: &egui::Context) {
    let preference = match settings.theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    };
    ctx.set_theme(preference);
    ctx.set_style_of(egui::Theme::Light, light_style());
    ctx.set_style_of(egui::Theme::Dark, dark_style());
}

/// Re-install only when the preference changed. The System dial follows the OS
/// on its own, so no per-frame work is needed afterwards.
pub fn apply_if_changed(
    settings: &crate::settings::Settings,
    ctx: &egui::Context,
    applied: &mut Theme,
) {
    if *applied != settings.theme {
        apply_theme(settings, ctx);
        *applied = settings.theme;
    }
}

/// The one primary button recipe: green fill, paired ink text.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(ACCENT_INK).strong())
            .fill(ACCENT)
            .stroke(Stroke::new(1.0, ACCENT_DEEP)),
    )
}

/// Section label: body face, a step smaller, sentence case, untracked.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().strong());
}

/// Muted helper line under a control or section.
pub fn helper(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

/// Pharmacy-label voice: shortcuts, model IDs, counts.
pub fn mono_small(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).monospace().small());
}

/// Error text: color plus words, never color alone.
pub fn error_label(ui: &mut Ui, text: &str) {
    ui.colored_label(ui.visuals().error_fg_color, text);
}

/// Success text in the theme's green.
pub fn success_label(ui: &mut Ui, text: &str) {
    ui.colored_label(SUCCESS, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_dials_carry_the_dose_tokens() {
        let light = light_style();
        assert!(!light.visuals.dark_mode);
        assert_eq!(light.visuals.panel_fill, PAPER);
        assert_eq!(light.visuals.window_fill, PAPER);

        let dark = dark_style();
        assert!(dark.visuals.dark_mode);
        assert_eq!(dark.visuals.panel_fill, NIGHT_PAPER);
    }

    #[test]
    fn body_stays_readable_and_controls_stay_in_density() {
        let style = light_style();
        let body = style.text_styles.get(&TextStyle::Body).expect("body style");
        assert!(body.size >= 15.0);
        assert!((style.spacing.interact_size.y - CONTROL_HEIGHT).abs() < f32::EPSILON);
    }
}
