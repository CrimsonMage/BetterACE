use eframe::egui::{self, Color32, FontId, RichText, Stroke, TextStyle};

pub(crate) const BG: Color32 = Color32::from_rgb(18, 23, 31);
pub(crate) const PANEL: Color32 = Color32::from_rgb(25, 32, 42);
pub(crate) const BORDER: Color32 = Color32::from_rgb(47, 59, 73);
pub(crate) const MUTED: Color32 = Color32::from_rgb(151, 167, 183);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(99, 211, 185);

pub(crate) fn apply(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(24.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(14.0));
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.text_edit_width = 220.0;
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(13, 18, 25);
    style.visuals.faint_bg_color = PANEL;
    style.visuals.override_text_color = Some(Color32::from_rgb(223, 231, 239));
    style.visuals.selection.bg_fill = Color32::from_rgb(35, 78, 76);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 44, 57);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(35, 44, 57);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 63, 76);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 63, 76);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(35, 78, 76);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub(crate) fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(10)
        .inner_margin(18)
}
pub(crate) fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(BG).strong())
        .fill(ACCENT)
        .min_size(egui::vec2(130.0, 36.0))
}
pub(crate) fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong().color(MUTED));
}
pub(crate) fn subtitle(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED));
}
pub(crate) fn notice(ui: &mut egui::Ui, text: &str) {
    if text.is_empty() {
        return;
    }
    ui.add_space(8.0);
    card().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(text);
    });
}
