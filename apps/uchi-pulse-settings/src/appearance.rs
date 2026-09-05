use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};
use std::{path::PathBuf, sync::Arc};

pub const INK: Color32 = Color32::from_rgb(35, 53, 58);
pub const MUTED: Color32 = Color32::from_rgb(95, 113, 118);
pub const ACCENT: Color32 = Color32::from_rgb(20, 112, 103);
pub const BACKGROUND: Color32 = Color32::from_rgb(245, 248, 247);
pub const BORDER: Color32 = Color32::from_rgb(216, 226, 223);

pub fn configure(ctx: &egui::Context) -> bool {
    let font_found = install_japanese_font(ctx);
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.selection.bg_fill = Color32::from_rgb(211, 235, 229);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(236, 243, 240);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(220, 237, 230);
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.interact_size.y = 36.0;
    style.spacing.text_edit_width = 360.0;
    for (kind, size) in [
        (TextStyle::Heading, 25.0),
        (TextStyle::Body, 16.0),
        (TextStyle::Button, 16.0),
        (TextStyle::Small, 13.0),
    ] {
        style
            .text_styles
            .insert(kind, FontId::new(size, FontFamily::Proportional));
    }
    ctx.set_style_of(egui::Theme::Light, style);
    font_found
}

fn install_japanese_font(ctx: &egui::Context) -> bool {
    let mut candidates = Vec::new();
    if let Some(user_dir) = std::env::var_os("HOME") {
        candidates
            .push(PathBuf::from(user_dir).join("Library/Fonts/NotoSansJP-VariableFont_wght.ttf"));
    }
    // Discover the actual filename: macOS stores these names in decomposed Unicode.
    if let Ok(entries) = std::fs::read_dir("/System/Library/Fonts") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("ヒラ") && name.ends_with(" W3.ttc") {
                candidates.push(entry.path());
            }
        }
    }
    candidates.extend(
        [
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "C:/Windows/Fonts/meiryo.ttc",
            "C:/Windows/Fonts/YuGothR.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ]
        .map(PathBuf::from),
    );
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "japanese".into(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "japanese".into());
        }
        ctx.set_fonts(fonts);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "macos")]
    fn japanese_labels_have_real_glyphs() {
        let ctx = eframe::egui::Context::default();
        assert!(
            super::configure(&ctx),
            "macOS Japanese font must be available"
        );
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.fonts_mut(|fonts| {
                assert!(fonts.has_glyphs(
                    &eframe::egui::FontId::proportional(16.0),
                    "家族・ボタンの動作・ご飯のお知らせ・保存しました"
                ));
            });
        });
    }
}
