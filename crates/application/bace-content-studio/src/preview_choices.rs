//! Choices taken from the selected table rather than guessed numeric identifiers.
use bace_content::ClothingPatchV1;
use eframe::egui;

pub(crate) fn clothing_choices(
    ui: &mut egui::Ui,
    table: &ClothingPatchV1,
    setup: &mut String,
    template: &mut u32,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Clothing variants");
        egui::ComboBox::from_id_salt("preview-setup-choice")
            .selected_text(format!("Setup {setup}"))
            .show_ui(ui, |ui| {
                for variant in &table.setups {
                    ui.selectable_value(
                        setup,
                        format!("{:08X}", variant.id),
                        format!("{:08X} · {} part changes", variant.id, variant.parts.len()),
                    );
                }
            });
        if table.palettes.is_empty() {
            ui.small("No palette templates in this table");
        } else {
            egui::ComboBox::from_id_salt("preview-template-choice")
                .selected_text(format!("Template {template}"))
                .show_ui(ui, |ui| {
                    for palette in &table.palettes {
                        ui.selectable_value(
                            template,
                            palette.template,
                            format!(
                                "{} · {} effects · icon {:08X}",
                                palette.template,
                                palette.effects.len(),
                                palette.icon
                            ),
                        );
                    }
                });
        }
    });
    ui.small("Choose a variant, then Load preview. Inspect DIDs first to include inherited DAT variants.");
}
