//! Mapping folio themes onto egui's style.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke, TextStyle};

use crate::config::{General, Rgba, Theme};

pub fn c(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn with_alpha(col: Rgba, a: u8) -> Color32 {
    let [r, g, b, _] = col.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn radius(t: &Theme) -> CornerRadius {
    CornerRadius::same(t.radius.clamp(0.0, 30.0) as u8)
}

pub fn apply(ctx: &egui::Context, theme: &Theme, general: &General) {
    let mut visuals = if theme.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = radius(theme);
    visuals.panel_fill = c(theme.surface);
    visuals.window_fill = c(theme.surface);
    visuals.extreme_bg_color = c(theme.surface_alt);
    visuals.text_edit_bg_color = Some(c(theme.surface_alt));
    visuals.faint_bg_color = c(theme.surface_alt);
    visuals.code_bg_color = c(theme.surface_alt);
    visuals.window_stroke = Stroke::new(1.0, c(theme.border));
    visuals.window_corner_radius = r;
    visuals.menu_corner_radius = r;
    visuals.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if theme.dark { 120 } else { 50 }) };
    visuals.popup_shadow = visuals.window_shadow;
    visuals.selection.bg_fill = with_alpha(theme.accent, 90);
    visuals.selection.stroke = Stroke::new(1.0, c(theme.accent));
    visuals.hyperlink_color = c(theme.accent);
    visuals.override_text_color = Some(c(theme.text));
    visuals.text_cursor.stroke = Stroke::new(2.0, c(theme.accent));
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same((theme.radius * 0.6) as u8);
        w.fg_stroke = Stroke::new(1.0, c(theme.text));
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c(theme.border));
    visuals.widgets.noninteractive.bg_fill = c(theme.surface);
    visuals.widgets.inactive.bg_fill = c(theme.surface_alt);
    visuals.widgets.inactive.weak_bg_fill = c(theme.surface_alt);
    visuals.widgets.hovered.bg_fill = with_alpha(theme.accent, 40);
    visuals.widgets.hovered.weak_bg_fill = with_alpha(theme.accent, 40);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, with_alpha(theme.accent, 120));
    visuals.widgets.active.bg_fill = with_alpha(theme.accent, 70);
    visuals.widgets.active.weak_bg_fill = with_alpha(theme.accent, 70);

    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        let size = general.ui_font_size;
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(size * 0.82)),
            (TextStyle::Body, FontId::proportional(size)),
            (TextStyle::Button, FontId::proportional(size)),
            (TextStyle::Heading, FontId::proportional(size * 1.3)),
            (TextStyle::Monospace, FontId::monospace(size * 0.95)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.scroll.floating = true;
        style.spacing.scroll.bar_width = 6.0;
    });
}

/// Resolve the interface font: the configured path, else Inter / Noto / Cantarell via fontconfig.
pub fn install_fonts(ctx: &egui::Context, general: &General) {
    let mut path = general.ui_font.clone();
    if path.is_none() {
        for family in ["Inter", "Inter Variable", "Noto Sans", "Cantarell", "IBM Plex Sans"] {
            let out = std::process::Command::new("fc-match").args(["-f", "%{family}\n%{file}", family]).output();
            if let Ok(out) = out {
                let s = String::from_utf8_lossy(&out.stdout);
                let mut lines = s.lines();
                let (fam, file) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
                if fam.split(',').any(|f| f.trim() == family) && !file.is_empty() {
                    path = Some(file.into());
                    break;
                }
            }
        }
    }
    let mut fonts = FontDefinitions::default();
    let mut mono_path = None;
    if let Ok(out) = std::process::Command::new("fc-match").args(["-f", "%{file}", "JetBrains Mono"]).output() {
        let f = String::from_utf8_lossy(&out.stdout).to_string();
        if f.to_lowercase().contains("jetbrains") {
            mono_path = Some(f);
        }
    }
    if let Some(bytes) = path.and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("ui".into(), FontData::from_owned(bytes).into());
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    if let Some(bytes) = mono_path.and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("mono".into(), FontData::from_owned(bytes).into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);
}
