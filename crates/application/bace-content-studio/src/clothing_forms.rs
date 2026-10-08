//! Structured editing of setup variants, ordered parts, textures and palette ranges.
use bace_content::*;
use eframe::egui;
#[derive(Default)]
pub(crate) struct ClothingForms {
    setup: usize,
    palette: usize,
    palette_mode: bool,
}
fn did(ui: &mut egui::Ui, label: &str, value: &mut u32) -> bool {
    ui.label(label);
    ui.add(
        egui::DragValue::new(value)
            .hexadecimal(8, false, true)
            .speed(1.0),
    )
    .changed()
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut u32) -> bool {
    ui.label(label);
    ui.add(egui::DragValue::new(value).speed(1.0)).changed()
}
impl ClothingForms {
    pub fn ui(&mut self, ui: &mut egui::Ui, patch: &mut ClothingPatchV1) -> bool {
        let mut changed = false;
        crate::theme::card().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                changed |= did(ui, "ClothingBase DID", &mut patch.id);
                ui.small("Hexadecimal · existing DAT IDs merge; new IDs are standalone overrides");
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.palette_mode,
                false,
                format!("Setup variants ({})", patch.setups.len()),
            );
            ui.selectable_value(
                &mut self.palette_mode,
                true,
                format!("Palette templates ({})", patch.palettes.len()),
            );
        });
        if self.palette_mode {
            changed |= self.palettes(ui, patch);
        } else {
            changed |= self.setups(ui, patch);
        }
        changed
    }
    fn setups(&mut self, ui: &mut egui::Ui, patch: &mut ClothingPatchV1) -> bool {
        let mut changed = false;
        ui.label("Each variant replaces that setup's complete part list. Add every model and texture change needed for that setup.");
        let mut remove = false;
        self.setup = self.setup.min(patch.setups.len().saturating_sub(1));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("clothing-setup")
                .selected_text(
                    patch
                        .setups
                        .get(self.setup)
                        .map(|s| format!("{:08X} · {} parts", s.id, s.parts.len()))
                        .unwrap_or_else(|| "No setup variants".into()),
                )
                .show_ui(ui, |ui| {
                    for (i, s) in patch.setups.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.setup,
                            i,
                            format!("{:08X} · {} parts", s.id, s.parts.len()),
                        );
                    }
                });
            if ui
                .add_enabled(patch.setups.len() < 4096, egui::Button::new("Add setup"))
                .clicked()
            {
                patch.setups.push(ClothingSetup {
                    id: 0x02000001,
                    parts: Vec::new(),
                });
                self.setup = patch.setups.len() - 1;
                changed = true;
            }
            if ui
                .add_enabled(
                    !patch.setups.is_empty() && patch.setups.len() < 4096,
                    egui::Button::new("Duplicate setup"),
                )
                .clicked()
            {
                let mut copy = patch.setups[self.setup].clone();
                copy.id = 0;
                patch.setups.push(copy);
                self.setup = patch.setups.len() - 1;
                changed = true;
            }
            remove = ui
                .add_enabled(!patch.setups.is_empty(), egui::Button::new("Remove setup"))
                .clicked();
        });
        if remove {
            patch.setups.remove(self.setup);
            self.setup = 0;
            return true;
        }
        let Some(setup) = patch.setups.get_mut(self.setup) else {
            return changed;
        };
        ui.push_id(self.setup, |ui| {
            ui.horizontal(|ui| {
                changed |= did(ui, "Setup DID", &mut setup.id);
                if ui
                    .add_enabled(
                        setup.parts.len() < 255,
                        egui::Button::new("Add part change"),
                    )
                    .clicked()
                {
                    let index = (0..=u8::MAX)
                        .find(|i| !setup.parts.iter().any(|p| p.index == *i))
                        .unwrap_or(0);
                    setup.parts.push(ClothingPart {
                        index,
                        model: 0x01000001,
                        textures: Vec::new(),
                    });
                    changed = true;
                }
            });
            let mut delete = None;
            let mut up = None;
            for (i, part) in setup.parts.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    crate::theme::card().show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(format!("Part change {}", i + 1));
                            ui.label("Index");
                            changed |= ui.add(egui::DragValue::new(&mut part.index)).changed();
                            changed |= did(ui, "Model DID", &mut part.model);
                            if ui.small_button("Remove part").clicked() {
                                delete = Some(i);
                            }
                            if i > 0 && ui.small_button("Move up").clicked() {
                                up = Some(i);
                            }
                        });
                        let mut remove_texture = None;
                        for (j, t) in part.textures.iter_mut().enumerate() {
                            ui.push_id(j, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    changed |= did(ui, "Old texture", &mut t.old);
                                    changed |= did(ui, "New texture", &mut t.new);
                                    if ui.small_button("Remove texture").clicked() {
                                        remove_texture = Some(j);
                                    }
                                });
                            });
                        }
                        if let Some(j) = remove_texture {
                            part.textures.remove(j);
                            changed = true;
                        }
                        if ui
                            .add_enabled(
                                part.textures.len() < 255,
                                egui::Button::new("Add texture change"),
                            )
                            .clicked()
                        {
                            part.textures.push(ClothingTexture {
                                old: 0x05000001,
                                new: 0x05000001,
                            });
                            changed = true;
                        }
                    });
                });
                ui.add_space(8.0);
            }
            if let Some(i) = delete {
                setup.parts.remove(i);
                changed = true;
            } else if let Some(i) = up {
                setup.parts.swap(i, i - 1);
                changed = true;
            }
        });
        changed
    }
    fn palettes(&mut self, ui: &mut egui::Ui, patch: &mut ClothingPatchV1) -> bool {
        let mut changed = false;
        ui.label("Each template can combine direct palettes (04), palette sets (0F), and multiple color ranges. Ranges use color counts in multiples of 8 (0 means 2048).");
        self.palette = self.palette.min(patch.palettes.len().saturating_sub(1));
        let mut remove = false;
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("clothing-template")
                .selected_text(
                    patch
                        .palettes
                        .get(self.palette)
                        .map(|p| format!("Template {} · {} effects", p.template, p.effects.len()))
                        .unwrap_or_else(|| "No palette templates".into()),
                )
                .show_ui(ui, |ui| {
                    for (i, p) in patch.palettes.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.palette,
                            i,
                            format!("Template {}", p.template),
                        );
                    }
                });
            if ui
                .add_enabled(
                    patch.palettes.len() < 4096,
                    egui::Button::new("Add template"),
                )
                .clicked()
            {
                let template = (0..=4096)
                    .find(|id| !patch.palettes.iter().any(|p| p.template == *id))
                    .unwrap_or(0);
                patch.palettes.push(ClothingPalette {
                    template,
                    icon: 0,
                    effects: Vec::new(),
                });
                self.palette = patch.palettes.len() - 1;
                changed = true;
            }
            remove = ui
                .add_enabled(
                    !patch.palettes.is_empty(),
                    egui::Button::new("Remove template"),
                )
                .clicked();
            if ui
                .add_enabled(
                    !patch.palettes.is_empty() && patch.palettes.len() < 4096,
                    egui::Button::new("Duplicate template"),
                )
                .clicked()
            {
                let template = (0..=4096)
                    .find(|id| !patch.palettes.iter().any(|p| p.template == *id))
                    .unwrap_or(0);
                let mut copy = patch.palettes[self.palette].clone();
                copy.template = template;
                patch.palettes.push(copy);
                self.palette = patch.palettes.len() - 1;
                changed = true;
            }
            if self.palette > 0
                && ui
                    .button("Move template up")
                    .on_hover_text("Template order controls the mod's first-template fallback")
                    .clicked()
            {
                patch.palettes.swap(self.palette, self.palette - 1);
                self.palette -= 1;
                changed = true;
            }
        });
        if remove {
            patch.palettes.remove(self.palette);
            self.palette = 0;
            return true;
        }
        let Some(palette) = patch.palettes.get_mut(self.palette) else {
            return changed;
        };
        ui.push_id(self.palette, |ui| {
            ui.horizontal_wrapped(|ui| {
                changed |= number(ui, "Template", &mut palette.template);
                changed |= did(ui, "Icon (0 = none)", &mut palette.icon);
                if ui
                    .add_enabled(
                        palette.effects.len() < 255,
                        egui::Button::new("Add palette effect"),
                    )
                    .clicked()
                {
                    palette.effects.push(ClothingPaletteEffect {
                        source: PaletteSource::Palette(0x04000001),
                        ranges: vec![ClothingRange {
                            offset: 0,
                            colors: 8,
                        }],
                    });
                    changed = true;
                }
            });
            let mut delete = None;
            let mut up = None;
            for (i, effect) in palette.effects.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    crate::theme::card().show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(format!("Palette effect {}", i + 1));
                            let mut direct = matches!(effect.source, PaletteSource::Palette(_));
                            let id = match &mut effect.source {
                                PaletteSource::Palette(id) | PaletteSource::PaletteSet(id) => id,
                            };
                            changed |= did(ui, "Palette / set DID", id);
                            let mut kind = false;
                            kind |= ui
                                .selectable_value(&mut direct, true, "Palette (04)")
                                .changed();
                            kind |= ui
                                .selectable_value(&mut direct, false, "Palette set (0F)")
                                .changed();
                            if kind {
                                effect.source = if direct {
                                    PaletteSource::Palette((*id & 0xffffff) | 0x04000000)
                                } else {
                                    PaletteSource::PaletteSet((*id & 0xffffff) | 0x0f000000)
                                };
                                changed = true;
                            }
                            if ui.small_button("Remove effect").clicked() {
                                delete = Some(i);
                            }
                            if i > 0 && ui.small_button("Move up").clicked() {
                                up = Some(i);
                            }
                        });
                        let mut remove_range = None;
                        for (j, r) in effect.ranges.iter_mut().enumerate() {
                            ui.push_id(j, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    changed |= number(ui, "Offset", &mut r.offset);
                                    changed |= number(ui, "Color count", &mut r.colors);
                                    if ui.small_button("Remove range").clicked() {
                                        remove_range = Some(j);
                                    }
                                });
                            });
                        }
                        if let Some(j) = remove_range {
                            effect.ranges.remove(j);
                            changed = true;
                        }
                        if ui
                            .add_enabled(effect.ranges.len() < 255, egui::Button::new("Add range"))
                            .clicked()
                        {
                            effect.ranges.push(ClothingRange {
                                offset: 0,
                                colors: 8,
                            });
                            changed = true;
                        }
                    });
                });
                ui.add_space(8.0);
            }
            if let Some(i) = delete {
                palette.effects.remove(i);
                changed = true;
            } else if let Some(i) = up {
                palette.effects.swap(i, i - 1);
                changed = true;
            }
        });
        changed
    }
}
