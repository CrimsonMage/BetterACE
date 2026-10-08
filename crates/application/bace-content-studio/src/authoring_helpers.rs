//! Offline authoring arithmetic, not a runtime gameplay implementation.
//! Formula: pinned ACE.Server/Entity/AttributeFormula.cs and FloatExtensions.Round.
use bace_content::WeenieV1;
use eframe::egui;
#[derive(Default)]
pub(crate) struct Helpers {
    target: u32,
    skill: u32,
    vital: usize,
    armor: i32,
    spell_chance: f32,
    notice: String,
}
impl Helpers {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        weenie: &mut WeenieV1,
        tables: Option<(&bace_dat::SkillTable, &bace_dat::VitalTable)>,
    ) -> bool {
        let mut changed = false;
        crate::theme::subtitle(
            ui,
            "Explicit authoring changes with undo. Calculations exclude player gear, enchantments and augmentations.",
        );
        crate::theme::card().show(ui,|ui| {
            ui.heading("Vitals & skills");
            if let Some((skills,vitals))=tables {
                ui.horizontal_wrapped(|ui| {
                    for (id,name) in ["Health","Stamina","Mana","Skill"].iter().enumerate(){ui.selectable_value(&mut self.vital,id,*name);}
                    ui.label("Desired base level");ui.add(egui::DragValue::new(&mut self.target).range(0..=1_000_000));
                });
                if self.vital==3 {
                    egui::ComboBox::from_label("Skill from DAT").selected_text(skills.skills.get(&self.skill).map_or("Choose a skill",|s|s.name.as_str())).show_ui(ui,|ui|{for (id,skill) in &skills.skills{ui.selectable_value(&mut self.skill,*id,&skill.name);}});
                }
                if ui.button("Calculate and set initial level").clicked() {
                    let formula=match self.vital {0=>Some(vitals.health),1=>Some(vitals.stamina),2=>Some(vitals.mana),_=>skills.skills.get(&self.skill).map(|s|s.formula)};
                    let result=formula.ok_or_else(||"Choose a DAT skill".to_string()).and_then(|f|attribute_bonus(weenie,f)).and_then(|bonus|{
                        if self.vital==3 {
                            let row=weenie.properties.skills.iter_mut().find(|p|i64::from(p.id)==i64::from(self.skill)).ok_or("Add this skill to the weenie first")?;
                            if row.value.sac<2 {return Err("This helper requires a trained or specialized skill".into());}
                            row.value.init_level=self.target.checked_sub(bonus).and_then(|n|n.checked_sub(u32::from(row.value.level_from_pp))).ok_or("Target is lower than the attribute bonus plus trained ranks")?;
                        } else {
                            let id=[1,3,5][self.vital];let row=weenie.properties.secondary_attributes.iter_mut().find(|p|p.id==id).ok_or("Add this vital to the weenie first")?;
                            row.value.init_level=self.target.checked_sub(bonus).and_then(|n|n.checked_sub(row.value.level_from_cp)).ok_or("Target is lower than the attribute bonus plus trained ranks")?;row.value.current_level=self.target;
                        }
                        Ok(bonus)
                    });
                    match result {Ok(bonus)=>{changed=true;self.notice=format!("Applied initial level; attributes contribute {bonus}.");},Err(e)=>self.notice=e,}
                }
            }else{ui.label("Load a DAT in 3D model & DIDs first. Skill and vital formulas come from your selected client assets.");}
        });
        ui.add_space(12.0);
        crate::theme::card().show(ui,|ui| {
            ui.heading("Body armor");ui.label("Set existing body-part armor values together. Hit locations and damage fields stay unchanged.");
            ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut self.armor).range(0..=100_000));if ui.button("Apply to all body parts").clicked(){for part in &mut weenie.properties.body_parts {let p=&mut part.value;p.base_armor=self.armor;p.armor_vs_slash=self.armor;p.armor_vs_pierce=self.armor;p.armor_vs_bludgeon=self.armor;p.armor_vs_cold=self.armor;p.armor_vs_fire=self.armor;p.armor_vs_acid=self.armor;p.armor_vs_electric=self.armor;p.armor_vs_nether=self.armor;}changed = !weenie.properties.body_parts.is_empty();}});
        });
        ui.add_space(12.0);
        crate::theme::card().show(ui,|ui| {
            ui.heading("Monster spell probabilities");
            ui.label("Sequential chances for the displayed spell-ID order. Runtime enumeration order must match for per-spell effective chances to agree.");
            let mut remaining=1.0_f64;
            for spell in &weenie.properties.spell_book {
                let chance=spell_probability(spell.value);let effective=remaining*f64::from(chance);remaining*=1.0-f64::from(chance);
                ui.label(format!("Spell {} · roll {:.2}% · effective {:.2}%",spell.id,chance*100.0,effective*100.0));
            }
            ui.strong(format!("Any spell: {:.2}% · no spell: {:.2}%",(1.0-remaining)*100.0,remaining*100.0));
            ui.horizontal(|ui|{ui.add(egui::Slider::new(&mut self.spell_chance,0.0..=100.0).text("Roll chance %"));if ui.button("Set all spell roll chances").clicked(){for spell in &mut weenie.properties.spell_book {spell.value=if self.spell_chance==0.0{0.0}else{2.0+self.spell_chance/100.0};}changed = !weenie.properties.spell_book.is_empty();}});
        });
        crate::theme::notice(ui, &self.notice);
        changed
    }
}
pub(crate) fn attribute_bonus(
    weenie: &WeenieV1,
    formula: bace_dat::SkillFormula,
) -> Result<u32, String> {
    if formula.x == 0 {
        return Ok(0);
    }
    if formula.z == 0 {
        return Err("DAT skill formula has a zero divisor".into());
    }
    let attribute = |id| {
        weenie
            .properties
            .attributes
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Missing attribute {id}"))
            .and_then(|a| {
                a.value
                    .init_level
                    .checked_add(a.value.level_from_cp)
                    .ok_or_else(|| "Attribute level overflow".into())
            })
    };
    let mut total = attribute(formula.attribute1)?;
    if formula.attribute2 != 0 {
        total = total
            .checked_add(attribute(formula.attribute2)?)
            .ok_or("Attribute sum overflow")?;
    }
    if total > 1_000_000 {
        return Err("Attribute sum exceeds authoring calculator limit".into());
    }
    Ok(if formula.z == 1 {
        total
    } else {
        (total as f32 / formula.z as f32).round() as u32
    })
}
// Pinned Monster_Magic.cs TryRollSpell: a literal 2.0 means 2%, not 0%.
pub(crate) fn spell_probability(value: f32) -> f32 {
    if value > 2.0 {
        (value - 2.0).clamp(0.0, 1.0)
    } else {
        (value / 100.0).clamp(0.0, 1.0)
    }
}
