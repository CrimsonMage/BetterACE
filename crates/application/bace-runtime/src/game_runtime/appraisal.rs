//! Bounded IdentifyObject ingress and private source-property response.
//! The full ACE dynamic assessment, creature roll and wake remain separate work.
use super::*;
use bace_simulation::{Command, ItemManaQueryOutcome, ItemManaQueryRequest, ItemManaValues};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use bace_wire::{
    AppraisalLimits, AppraisalProfile, AppraisalWeapon, GameActionEnvelope, IdentifyObjectRequest,
};
use std::collections::BTreeMap;
use std::sync::mpsc::TrySendError;

#[derive(Default)]
pub(super) struct AppraisalRuntime {
    pending: BTreeMap<u64, PendingAppraisal>,
    by_session: BTreeMap<SessionKey, u64>,
    retained: Option<ItemManaQueryOutcome>,
    unexpected: Option<ItemManaQueryOutcome>,
}

struct PendingAppraisal {
    key: SessionKey,
    binding: bace_gameplay_api::CharacterBinding,
    target: EntityId,
    profile: AppraisalProfile,
}

impl AppraisalRuntime {
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || self.retained.is_some() || self.unexpected.is_some()
    }
}

impl GameRuntime {
    pub(super) fn handle_appraisal_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let envelope = match GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes) {
            Ok(value) => value,
            Err(_) => return Ok(I::Unsupported),
        };
        if envelope.action != bace_wire::opcode::GameActionType::IdentifyObject {
            return Ok(I::Unsupported);
        }
        let request = IdentifyObjectRequest::decode(envelope.payload)
            .map_err(|error| format!("IdentifyObject request: {error:?}"))?;
        if self.appraisal.by_session.contains_key(&key) {
            return Ok(I::Blocked);
        }
        let Some(loading) = self
            .sessions
            .get(&key)
            .and_then(|session| session.loading.as_ref())
        else {
            return Ok(I::Blocked);
        };
        let binding = loading.loaded.binding;
        if !self.players.entered(binding.actor) {
            return Ok(I::Blocked);
        }
        // ACE Player.HandleActionIdentifyObject clears selection without output.
        if request.target == 0 {
            return Ok(I::Accepted);
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(I::Blocked);
        }
        let target = EntityId(request.target);
        let owned_item = target != binding.actor
            && self
                .online_saves
                .inventory_baseline(binding.actor.0, target.0)
                .is_some();
        let (profile, found) = {
            let owned = if target == binding.actor {
                Some(&loading.loaded.player.player.entity.state)
            } else {
                self.online_saves
                    .inventory_baseline(binding.actor.0, target.0)
                    .map(|item| &item.entity.state)
            };
            let source = owned.or_else(|| {
                if !self.visibility.service.knows(key, target) {
                    return None;
                }
                self.online_saves
                    .item_baseline(target.0)
                    .map(|(_, item)| &item.entity.state)
                    .or_else(|| self.npc.appraisal_source(target))
                    .or_else(|| {
                        self.world
                            .as_ref()
                            .and_then(|world| world.regions.appraisal_source(target))
                    })
            });
            (
                source.map_or_else(AppraisalProfile::default, source_profile),
                source.is_some(),
            )
        };
        self.diagnostic(
            "info",
            "game.identify",
            &format!(
                "session {} generation {}: target {:#010x}, found={}",
                key.id, key.generation, target.0, found
            ),
        );
        if owned_item && needs_live_item_mana(&profile) {
            if self.appraisal.pending.len() >= self.limits.messages {
                return Ok(I::Blocked);
            }
            let token = self.token()?;
            match self
                .simulation
                .input()
                .try_submit(Command::ItemManaQuery(ItemManaQueryRequest {
                    correlation: token,
                    binding,
                    target,
                })) {
                Ok(()) => {
                    self.appraisal.pending.insert(
                        token,
                        PendingAppraisal {
                            key,
                            binding,
                            target,
                            profile,
                        },
                    );
                    self.appraisal.by_session.insert(key, token);
                    return Ok(I::Accepted);
                }
                Err(TrySendError::Full(_)) => return Ok(I::Blocked),
                Err(TrySendError::Disconnected(_)) => {
                    return Err("IdentifyObject mana query simulation closed".into());
                }
            }
        }
        self.send_appraisal(key, binding, target, &profile)?;
        Ok(I::Accepted)
    }

    pub(super) fn poll_appraisal(&mut self) -> Result<(), String> {
        if self.appraisal.unexpected.is_some() {
            return Err("unmatched item mana query retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let outcome = match self.appraisal.retained.take() {
                Some(value) => value,
                None => match self.simulation.item_mana_query_outcomes().try_recv() {
                    Ok(value) => value,
                    Err(_) => break,
                },
            };
            let Some(pending) = self.appraisal.pending.get(&outcome.correlation) else {
                self.appraisal.unexpected = Some(outcome);
                return Err("unmatched item mana query retained".into());
            };
            let detached = self.sessions.get(&pending.key).is_none_or(|session| {
                session.disconnected
                    || session
                        .loading
                        .as_ref()
                        .is_none_or(|loading| loading.loaded.binding != pending.binding)
            });
            if !detached && self.network_output.len() >= self.limits.messages {
                self.appraisal.retained = Some(outcome);
                break;
            }
            let pending = self
                .appraisal
                .pending
                .remove(&outcome.correlation)
                .expect("matched query");
            self.appraisal.by_session.remove(&pending.key);
            if detached {
                self.diagnostic(
                    "info",
                    "game.identify_cancelled",
                    &format!(
                        "target {:#010x}: session detached before mana response",
                        pending.target.0
                    ),
                );
                continue;
            }
            let mut profile = pending.profile;
            match outcome.result {
                Ok(Some(values)) => overlay_item_mana(&mut profile, values),
                Ok(None) => {}
                Err(error) => {
                    self.diagnostic(
                        "warn",
                        "game.identify_stale",
                        &format!("target {:#010x}: {error:?}", pending.target.0),
                    );
                    profile = AppraisalProfile::default();
                }
            }
            self.send_appraisal(pending.key, pending.binding, pending.target, &profile)?;
        }
        Ok(())
    }

    fn send_appraisal(
        &mut self,
        key: SessionKey,
        binding: bace_gameplay_api::CharacterBinding,
        target: EntityId,
        profile: &AppraisalProfile,
    ) -> Result<(), String> {
        let codec = AppraisalLimits {
            table_entries: 1024,
            string_bytes: 4096,
            message_bytes: self.limits.message_bytes,
        };
        let limits = bace_replication::BatchLimits {
            max_messages: 1,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4096,
        };
        let recipient = self
            .players
            .replication(binding.actor)
            .ok_or("IdentifyObject recipient missing")?;
        if recipient.key != key || recipient.binding != binding {
            return Err("IdentifyObject binding mismatch".into());
        }
        let batch = recipient
            .events
            .project_appraisal(binding, target.0, profile, codec, limits)
            .map_err(|error| format!("IdentifyObject response: {error:?}"))?;
        self.diagnostic(
            "info",
            "game.identify_response",
            &format!(
                "session {} generation {}: target {:#010x}, queue 9, event 0x00c9, sequence {}, bytes {}",
                key.id,
                key.generation,
                target.0,
                u32::from_le_bytes(batch.messages[0].bytes[8..12].try_into().expect("event stamp")),
                batch.messages[0].bytes.len()
            ),
        );
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch)
                .map_err(|error| error.to_string())?,
        );
        Ok(())
    }
}

fn overlay_item_mana(profile: &mut AppraisalProfile, values: ItemManaValues) {
    for (id, value) in [(107, values.current), (108, values.maximum)] {
        if let Some(value) = value {
            set_int(&mut profile.integers, id, value);
        } else {
            profile.integers.retain(|(key, _)| *key != id);
        }
    }
}

fn needs_live_item_mana(profile: &AppraisalProfile) -> bool {
    profile
        .integers
        .iter()
        .any(|(id, _)| matches!(*id, 107 | 108))
}

fn source_profile(source: &bace_content::WeenieV1) -> AppraisalProfile {
    // ACE AppraiseInfo.BuildProperties first applies the AssessmentProperty
    // allowlists. These property tables and BuildSpells' primary/proc/source
    // spell order are immutable; dynamic effects are not inferred from a DTO.
    let mut selected = bace_simulation::AppraisalSourceTables::select(source);
    // ACE AppraiseInfo.AddPropertyEnchantments obtains ItemSkillLimit from
    // PropertyDataId 37 and sends it as appraisal-only PropertyInt 176.
    // Live enchantments are deliberately not inferred from immutable source.
    if let Some(skill) = data_id(source, 37).and_then(|value| i32::try_from(value).ok()) {
        set_int(&mut selected.integers, 176, skill);
    } else {
        selected.integers.retain(|(id, _)| *id != 176);
    }
    // BuildProfile decorates LongDesc only when it begins with the item name.
    let decoration = (u8::from(int(source, 105).is_some_and(|value| value > 0)))
        | (u8::from(int(source, 131).is_some_and(|value| value > 0)) << 1)
        | (u8::from(
            int(source, 178).is_some_and(|value| value > 0)
                && int(source, 177).is_some_and(|value| value > 0),
        ) << 2);
    if decoration != 0
        && string(source, 16)
            .zip(string(source, 1))
            .is_some_and(|(description, name)| description.starts_with(name))
    {
        set_int(&mut selected.integers, 172, i32::from(decoration));
    } else {
        selected.integers.retain(|(id, _)| *id != 172);
    }
    // ACE removes a portal's burden from its examination panel.
    if source.weenie_type == 7 {
        selected.integers.retain(|(id, _)| *id != 5);
    }
    // ACE AppraiseInfo.BuildProfile overrides ManaStone's Use string according
    // to whether it currently contains ItemCurMana, independent of its value.
    if source.weenie_type == 37 {
        set_string(
            &mut selected.strings,
            14,
            if int(source, 107).is_some() {
                "Use on a magic item to give the stone's stored Mana to that item."
            } else {
                "Use on a magic item to destroy that item and drain its Mana."
            }
            .into(),
        );
    }
    let mut spells = Vec::new();
    if source.weenie_type != 10 {
        for id in [28, 55] {
            if let Some(row) = source.properties.data_ids.iter().find(|row| row.id == id)
                && row.value != 0
            {
                spells.push(row.value);
            }
        }
        for row in &source.properties.spell_book {
            if let Ok(id) = u32::try_from(row.id)
                && id != 0
                && ![data_id(source, 28), data_id(source, 55)]
                    .into_iter()
                    .flatten()
                    .any(|primary| primary == id)
            {
                spells.push(id);
            }
        }
    }
    // ACE AppraiseInfo.BuildArmor includes an ArmorProfile for Clothing.
    // ArmorProfile.GetArmorMod reads these eight authored base modifiers in
    // protocol order, defaulting each missing modifier to 1.0. Dynamic bane
    // enchantments need the live enchantment owner and are not source data.
    let armor = (source.weenie_type == 2 || int(source, 51) == Some(4)).then(|| {
        [13, 14, 15, 16, 17, 18, 165, 19].map(|id| {
            source
                .properties
                .floats
                .iter()
                .find(|row| row.id == id)
                .map_or(1.0, |row| row.value.clamp(-2.0, 2.0) as f32)
        })
    });
    // ACE BuildWeapon creates this section for damaging objects and the
    // concrete weapon classes, except Caster (which only contributes defense
    // and enchantment presentation). GetDamage/Speed clamp negative bases to
    // zero; GetDamageVariance/Multiplier/Offense return float before their
    // values are stored as doubles in WeaponProfile.
    let weapon = ((int(source, 44).is_some() && source.weenie_type != 2)
        || matches!(source.weenie_type, 3..=6))
    .then(|| {
        (source.weenie_type != 35).then(|| AppraisalWeapon {
            damage_type: int(source, 45).unwrap_or(0) as u32,
            time: int(source, 49).unwrap_or(0).max(0) as u32,
            skill: int(source, 48).unwrap_or(0) as u32,
            damage: int(source, 44).unwrap_or(0).max(0) as u32,
            variance: f64::from(float(source, 22).unwrap_or(0.0) as f32),
            damage_modifier: f64::from(float(source, 63).unwrap_or(1.0) as f32),
            length: float(source, 21).unwrap_or(1.0),
            maximum_velocity: float(source, 26).unwrap_or(1.0),
            offense: f64::from(if source.weenie_type == 5 {
                1.0
            } else {
                float(source, 62).unwrap_or(1.0)
            } as f32),
            velocity_estimated: 0,
        })
    })
    .flatten();
    AppraisalProfile {
        success: true,
        integers: selected.integers,
        integers64: selected.integers64,
        booleans: selected.booleans,
        doubles: selected.doubles,
        strings: selected.strings,
        data_ids: selected.data_ids,
        spells,
        armor,
        weapon,
        ..Default::default()
    }
}

fn int(source: &bace_content::WeenieV1, id: u32) -> Option<i32> {
    source
        .properties
        .ints
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.value)
}

fn float(source: &bace_content::WeenieV1, id: u32) -> Option<f64> {
    source
        .properties
        .floats
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.value)
}

fn data_id(source: &bace_content::WeenieV1, id: u32) -> Option<u32> {
    source
        .properties
        .data_ids
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.value)
}

fn string(source: &bace_content::WeenieV1, id: u32) -> Option<&str> {
    source
        .properties
        .strings
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.value.as_str())
}

fn set_int(entries: &mut Vec<(u16, i32)>, id: u16, value: i32) {
    if let Some((_, old)) = entries.iter_mut().find(|(key, _)| *key == id) {
        *old = value;
    } else {
        entries.push((id, value));
    }
}

fn set_string(entries: &mut Vec<(u16, String)>, id: u16, value: String) {
    if let Some((_, old)) = entries.iter_mut().find(|(key, _)| *key == id) {
        *old = value;
    } else {
        entries.push((id, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::Property;

    #[test]
    fn pinned_source_properties_and_spell_order_enter_response() {
        // ACE AppraiseInfo.BuildProperties/BuildSpells: only annotated source
        // properties, then primary Spell, ProcSpell and remaining known spells.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 123,
            class_name: "appraisal-source".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.ints.extend([
            Property { id: 25, value: 10 },
            Property { id: 67, value: 2 },
        ]);
        source.properties.data_ids.extend([
            Property { id: 28, value: 100 },
            Property { id: 55, value: 200 },
        ]);
        source.properties.spell_book.extend([
            Property { id: 100, value: 1. },
            Property { id: 300, value: 1. },
        ]);
        let profile = source_profile(&source);
        assert_eq!(profile.integers, vec![(25, 10)]);
        assert_eq!(profile.data_ids, vec![(55, 200)]);
        assert_eq!(profile.spells, vec![100, 200, 300]);
    }

    #[test]
    fn pinned_clothing_armor_profile_uses_ace_modifier_order_and_defaults() {
        // ACE AppraiseInfo.BuildArmor and ArmorProfile.GetArmorMod, at the
        // pinned baseline: clothing uses authored ArmorModVs* properties.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 124,
            class_name: "clothing".into(),
            weenie_type: 2,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.floats.extend([
            Property { id: 13, value: 0.7 },
            Property {
                id: 165,
                value: 3.0,
            },
            Property {
                id: 19,
                value: -3.0,
            },
        ]);
        assert_eq!(
            source_profile(&source).armor,
            Some([0.7, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, -2.0])
        );
    }

    #[test]
    fn pinned_weapon_profile_supplies_item_panel_details() {
        // ACE AppraiseInfo.BuildWeapon and WeaponProfile.Get* use these
        // exact source fields. The float-returning helpers round once before
        // the profile writer emits their doubles.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 125,
            class_name: "training-sword".into(),
            weenie_type: 6,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.ints.extend([
            Property { id: 45, value: 1 },
            Property { id: 49, value: 32 },
            Property { id: 48, value: 44 },
            Property { id: 44, value: 11 },
        ]);
        source.properties.floats.extend([
            Property {
                id: 22,
                value: 0.375,
            },
            Property { id: 63, value: 1.5 },
            Property {
                id: 21,
                value: 2.25,
            },
            Property {
                id: 26,
                value: 30.0,
            },
            Property {
                id: 62,
                value: 1.125,
            },
        ]);
        let profile = source_profile(&source);
        assert_eq!(
            profile.weapon,
            Some(AppraisalWeapon {
                damage_type: 1,
                time: 32,
                skill: 44,
                damage: 11,
                variance: 0.375,
                damage_modifier: 1.5,
                length: 2.25,
                maximum_velocity: 30.0,
                offense: 1.125,
                velocity_estimated: 0,
            })
        );
        let bytes = profile
            .encode(
                0x5000_0001,
                7,
                0x8000_0001,
                AppraisalLimits {
                    table_entries: 1024,
                    string_bytes: 4096,
                    message_bytes: 4096,
                },
            )
            .unwrap();
        assert_eq!(&bytes[20..24], &0x21u32.to_le_bytes());
        assert_eq!(&bytes[28..30], &1u16.to_le_bytes());
        assert_eq!(&bytes[30..32], &16u16.to_le_bytes());
        assert_eq!(&bytes[32..36], &45u32.to_le_bytes());
        assert_eq!(&bytes[40..44], &1u32.to_le_bytes());
        assert_eq!(&bytes[52..56], &11u32.to_le_bytes());
    }

    #[test]
    fn pinned_shield_and_item_decoration_are_projected() {
        // ACE WorldObject.IsShield checks CombatUse 4. AppraiseInfo
        // AddPropertyEnchantments adds Int 176 from DID 37 and Int 172 from
        // workmanship/material/gem metadata only for name-prefixed LongDesc.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 126,
            class_name: "shield".into(),
            weenie_type: 6,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.ints.extend([
            Property { id: 51, value: 4 },
            Property { id: 105, value: 5 },
            Property { id: 131, value: 2 },
            Property { id: 177, value: 1 },
            Property { id: 178, value: 3 },
        ]);
        source
            .properties
            .data_ids
            .push(Property { id: 37, value: 44 });
        source.properties.strings.extend([
            Property {
                id: 1,
                value: "Shield".into(),
            },
            Property {
                id: 16,
                value: "Shield of testing".into(),
            },
        ]);
        let profile = source_profile(&source);
        assert_eq!(profile.armor, Some([1.0; 8]));
        assert!(profile.integers.contains(&(172, 7)));
        assert!(profile.integers.contains(&(176, 44)));
        source.properties.strings[1].value = "Unrelated description".into();
        assert!(
            !source_profile(&source)
                .integers
                .iter()
                .any(|(id, _)| *id == 172)
        );
    }

    #[test]
    fn pinned_caster_and_ammunition_profile_exceptions() {
        // ACE BuildWeapon constructs a temporary caster WeaponProfile but
        // deliberately does not send the WeaponProfile section. Ammunition
        // always reports an offense multiplier of 1.0.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 127,
            class_name: "caster".into(),
            weenie_type: 35,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.ints.push(Property { id: 44, value: 7 });
        assert_eq!(source_profile(&source).weapon, None);
        source.weenie_type = 5;
        source
            .properties
            .floats
            .push(Property { id: 62, value: 3.0 });
        assert_eq!(source_profile(&source).weapon.unwrap().offense, 1.0);
    }

    #[test]
    fn pinned_item_mana_and_use_limits_remain_visible() {
        // ACE AssessmentProperty annotations on Int107/108/115/117,
        // Int158-160 and Float5/144 feed the item details panel. The source
        // profile must preserve their values; changing current mana later is
        // owned by the live equipment-mana snapshot, not this projection.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 128,
            class_name: "mana-item".into(),
            weenie_type: 35,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.ints.extend([
            Property { id: 91, value: 10 },
            Property { id: 92, value: 3 },
            Property { id: 107, value: 55 },
            Property {
                id: 108,
                value: 100,
            },
            Property {
                id: 115,
                value: 250,
            },
            Property { id: 117, value: 3 },
            Property { id: 158, value: 1 },
            Property { id: 159, value: 34 },
            Property {
                id: 160,
                value: 275,
            },
        ]);
        source.properties.floats.extend([
            Property {
                id: 5,
                value: -0.05,
            },
            Property {
                id: 144,
                value: 1.15,
            },
        ]);
        let profile = source_profile(&source);
        for value in [
            (91, 10),
            (92, 3),
            (107, 55),
            (108, 100),
            (115, 250),
            (117, 3),
            (158, 1),
            (159, 34),
            (160, 275),
        ] {
            assert!(profile.integers.contains(&value), "missing {value:?}");
        }
        assert!(profile.doubles.contains(&(5, -0.05)));
        assert!(profile.doubles.contains(&(144, 1.15)));
    }

    #[test]
    fn pinned_mana_stone_use_description_follows_current_mana_presence() {
        // ACE AppraiseInfo.BuildProfile, ManaStone branch.
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 129,
            class_name: "mana-stone".into(),
            weenie_type: 37,
            last_modified: None,
            properties: Default::default(),
        };
        assert_eq!(
            source_profile(&source).strings,
            vec![(
                14,
                "Use on a magic item to destroy that item and drain its Mana.".into()
            )]
        );
        source.properties.ints.push(Property { id: 107, value: 0 });
        assert_eq!(
            source_profile(&source).strings,
            vec![(
                14,
                "Use on a magic item to give the stone's stored Mana to that item.".into()
            )]
        );
    }

    #[test]
    fn live_equipment_mana_replaces_saved_appraisal_values() {
        let mut profile = AppraisalProfile {
            success: true,
            integers: vec![(107, 90), (108, 100), (115, 250)],
            ..Default::default()
        };
        overlay_item_mana(
            &mut profile,
            ItemManaValues {
                current: Some(43),
                maximum: Some(105),
            },
        );
        assert_eq!(profile.integers, vec![(107, 43), (108, 105), (115, 250)]);
        overlay_item_mana(
            &mut profile,
            ItemManaValues {
                current: None,
                maximum: Some(105),
            },
        );
        assert_eq!(profile.integers, vec![(108, 105), (115, 250)]);
    }

    #[test]
    fn ordinary_item_details_do_not_require_simulation_mana_query() {
        let mut source = bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 130,
            class_name: "ordinary-item".into(),
            weenie_type: 6,
            last_modified: None,
            properties: Default::default(),
        };
        source.properties.ints.push(Property { id: 44, value: 7 });
        assert!(!needs_live_item_mana(&source_profile(&source)));
        source.properties.ints.push(Property { id: 107, value: 0 });
        assert!(needs_live_item_mana(&source_profile(&source)));
    }
}
