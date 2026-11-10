//! Shared workbench tokens, aligned with PhotoCraft's Pro family. This module
//! owns all UI colours and metrics; it does not read or change host preferences.
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use natural_skin_companion::desktop::Appearance;

pub const INSPECTOR_WIDTH: f32 = 300.0;
pub const INSET: f32 = 12.0;
pub const SECTION_GAP: f32 = 16.0;
pub const ACCENT: Color32 = Color32::from_rgb(55, 142, 240);

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/Inter-Regular.ttf")).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    // Keep bundled and system fallbacks for CJK filenames and the Chinese UI.
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, appearance: Appearance) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let light = theme == egui::Theme::Light;
        let gray = appearance == Appearance::Gray && !light;
        let panel = Color32::from_gray(if light {
            240
        } else if gray {
            83
        } else {
            50
        });
        let field = Color32::from_gray(if light {
            255
        } else if gray {
            69
        } else {
            36
        });
        let border = Color32::from_gray(if light {
            186
        } else if gray {
            104
        } else {
            90
        });
        let separator = Color32::from_gray(if light {
            214
        } else if gray {
            62
        } else {
            30
        });
        let text = Color32::from_gray(if light {
            30
        } else if gray {
            238
        } else {
            222
        });
        let secondary = Color32::from_gray(if light {
            80
        } else if gray {
            212
        } else {
            188
        });
        // egui's built-in numeric help is English-only; adjustments provide
        // equivalent application-owned help in the selected language.
        let mut style = egui::Style {
            explanation_tooltips: false,
            ..Default::default()
        };
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.interact_size = egui::vec2(32.0, 26.0);
        style.spacing.slider_width = 180.0;
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(15.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.0, FontFamily::Monospace),
            ),
        ]
        .into();
        let mut visuals = if light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        visuals.panel_fill = panel;
        visuals.window_fill = panel;
        visuals.extreme_bg_color = field;
        visuals.faint_bg_color = field;
        visuals.text_edit_bg_color = Some(field);
        visuals.override_text_color = Some(text);
        visuals.weak_text_color = Some(secondary);
        visuals.selection.bg_fill = if light {
            Color32::from_rgb(205, 224, 247)
        } else {
            Color32::from_rgb(45, 76, 112)
        };
        visuals.selection.stroke = Stroke::new(
            1.0,
            if light {
                Color32::from_rgb(20, 79, 150)
            } else {
                Color32::WHITE
            },
        );
        visuals.hyperlink_color = if light {
            Color32::from_rgb(20, 88, 170)
        } else {
            Color32::from_rgb(120, 185, 255)
        };
        visuals.error_fg_color = if light {
            Color32::from_rgb(175, 32, 40)
        } else {
            Color32::from_rgb(255, 170, 170)
        };
        visuals.slider_trailing_fill = true;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.bg_fill = field;
            widget.weak_bg_fill = field;
            widget.bg_stroke = Stroke::new(1.0, border);
            widget.fg_stroke = Stroke::new(1.0, text);
            widget.corner_radius = CornerRadius::same(4);
            widget.expansion = 0.0;
        }
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, separator);
        visuals.widgets.hovered.bg_fill = Color32::from_gray(if light {
            226
        } else if gray {
            98
        } else {
            66
        });
        visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        visuals.widgets.active.bg_fill = Color32::from_gray(if light {
            206
        } else if gray {
            112
        } else {
            78
        });
        visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
        visuals.widgets.open = visuals.widgets.active;
        visuals.window_corner_radius = CornerRadius::same(6);
        visuals.menu_corner_radius = CornerRadius::same(6);
        style.visuals = visuals;
        ctx.set_style_of(theme, style);
    }
    ctx.set_theme(match appearance {
        Appearance::Dark | Appearance::Gray => egui::ThemePreference::Dark,
        Appearance::Light => egui::ThemePreference::Light,
        Appearance::System => egui::ThemePreference::System,
    });
}

pub fn canvas() -> Color32 {
    Color32::from_gray(40)
}
pub fn strip(ui: &mut egui::Ui, title: impl Into<egui::RichText>) {
    let fill = if ui.visuals().dark_mode {
        ui.visuals().extreme_bg_color
    } else {
        Color32::from_gray(226)
    };
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 20.0).max(0.0));
            ui.strong(title);
        });
    ui.add_space(8.0);
}
pub fn primary(text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(text.into().color(Color32::WHITE))
        .fill(Color32::from_rgb(22, 100, 188))
        .stroke(Stroke::new(1.0, ACCENT))
        .corner_radius(12)
}
