//! Publish only accepted snapshot state using the canonical session counters.
use super::*;
impl GameRuntime {
    pub(super) fn poll_durability(&mut self, elapsed: Duration, unix: u64) -> Result<(), String> {
        self.online_saves.poll_writes(self.limits.work_per_poll)?;
        if let Some(outcome) = ready(&mut self.resolution) {
            self.online_saves.accept_resolution(outcome)?;
        }
        if self.resolution.is_none()
            && let Some(request) = self.online_saves.resolution_request()
        {
            let store = self.bootstrap.store.clone();
            self.resolution = Some(Box::pin(async move { request.resolve(&store).await }));
        }
        if self.unexpected_snapshot.is_some() {
            return Err("unrelated player snapshot retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.player_snapshot_outcomes().try_recv() else {
                break;
            };
            if self
                .snapshot
                .is_some_and(|(token, _)| token == outcome.correlation)
            {
                let (_, key) = self.snapshot.take().expect("matched snapshot");
                let session = self
                    .sessions
                    .get_mut(&key)
                    .ok_or("snapshot player missing")?;
                match outcome.result {
                    Ok(snapshot) => {
                        let loading = session.loading.as_mut().ok_or("snapshot loading missing")?;
                        if snapshot.binding() != loading.loaded.binding {
                            return Err("entry snapshot identity mismatch".into());
                        }
                        loading.snapshot = Some(snapshot);
                        loading.phase = lifecycle::Phase::Entry;
                    }
                    Err(error) => session.failure = Some(format!("entry snapshot: {error:?}")),
                }
            } else if self.staff_owns_capture(&outcome) {
                self.accept_staff_capture(outcome, unix)?;
            } else if self.allegiance.service.owns_capture(&outcome) {
                self.allegiance
                    .service
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "allegiance snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.progression.service.owns_capture(&outcome) {
                self.progression
                    .service
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "skill snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.portals.service.owns_capture(&outcome) {
                self.portals
                    .service
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "portal snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.deaths.owns_capture(&outcome) {
                if let Err(outcome) = self.deaths.accept_capture(outcome, unix) {
                    self.unexpected_snapshot = Some(outcome);
                    return Err("unmatched player death capture retained".into());
                }
            } else if self.pve_deaths.owns_capture(&outcome) {
                if let Err(outcome) = self.accept_pve_capture(outcome, unix) {
                    self.unexpected_snapshot = Some(outcome);
                    return Err("unmatched PVE death capture retained".into());
                }
            } else if self.pets.service.owns_capture(&outcome) {
                if let Err(outcome) = self.pets.service.accept_capture(outcome, unix) {
                    self.unexpected_snapshot = Some(outcome);
                    return Err("unmatched pet capture retained".into());
                }
            } else if self.pets.owns_capture(&outcome) {
                if let Err(outcome) = self.pets.accept_capture(outcome) {
                    self.unexpected_snapshot = Some(outcome);
                    return Err("unmatched pet Use capture retained".into());
                }
            } else if self.skill_devices.owns_capture(&outcome) {
                self.skill_devices
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "device snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.attribute_transfers.owns_capture(&outcome) {
                self.attribute_transfers
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "attribute transfer snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.recalls.owns_capture(&outcome) {
                self.recalls
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "recall snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.crafting.owns_capture(&outcome) {
                self.crafting
                    .accept_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "crafting snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.inventory_owns_capture(&outcome) {
                self.accept_inventory_capture(outcome, unix)
                    .map_err(|outcome| {
                        self.unexpected_snapshot = Some(outcome);
                        "inventory snapshot correlation mismatch".to_owned()
                    })?;
            } else if self.magic_owns_capture(&outcome) {
                self.accept_magic_capture(outcome, unix)?;
            } else if self.npc_owns_capture(&outcome) {
                self.accept_npc_capture(outcome, unix).map_err(|outcome| {
                    self.unexpected_snapshot = Some(outcome);
                    "NPC snapshot correlation mismatch".to_owned()
                })?;
            } else if !self.online_saves.owns_capture(outcome.correlation) {
                self.unexpected_snapshot = Some(outcome);
                return Err("unrelated snapshot outcome retained".into());
            } else if let Err((error, outcome)) = self.online_saves.accept_capture(outcome, unix) {
                // A capture owned by this controller may defer on valuable holds;
                // it retains dirty age. Only truly unrelated tokens are retained.
                if outcome.result.is_ok() {
                    self.unexpected_snapshot = Some(outcome);
                    return Err(error);
                }
            }
        }
        self.online_saves.submit_due(
            &self.saves.handle,
            elapsed,
            self.draining,
            self.limits.work_per_poll,
        )?;
        if self.snapshot.is_none() {
            let token = self.token()?;
            self.online_saves.request_capture(
                &self.simulation.input(),
                token,
                elapsed,
                self.draining,
            )?;
        }
        Ok(())
    }
    pub(super) fn prepare_entry(&mut self, key: SessionKey, unix: u64) -> Result<(), String> {
        if !self.players.network_ready() {
            return Ok(());
        }
        let loading = self
            .sessions
            .get_mut(&key)
            .ok_or("entry session missing")?
            .loading
            .as_mut()
            .ok_or("entry loading missing")?;
        let physics = loading
            .snapshot
            .as_ref()
            .ok_or("entry snapshot missing")?
            .entry_physics();
        if !physics.grounded() || physics.velocity() != bace_geometry::Vec3::ZERO {
            if loading.settle_attempts >= 300 {
                return Err("entry physics did not settle after 300 accepted snapshots".into());
            }
            loading.settle_attempts += 1;
            loading.snapshot = None;
            loading.phase = lifecycle::Phase::Snapshot;
            return Ok(());
        }
        let loading = self.sessions[&key]
            .loading
            .as_ref()
            .ok_or("entry loading missing")?;
        let snapshot = loading.snapshot.as_ref().ok_or("entry snapshot missing")?;
        let source = &loading.loaded.player;
        let saved = crate::player_saves::freeze_player_snapshot(source, snapshot, unix)
            .map_err(|e| e.to_string())?;
        let appearance = loading
            .appearance
            .as_ref()
            .ok_or("entry appearance missing")?;
        let setup_id = saved
            .player
            .entity
            .state
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 1)
            .ok_or("entry setup id missing")?
            .value;
        let setup = appearance
            .setups
            .get(&setup_id)
            .ok_or("entry verified setup missing")?;
        let replica = self
            .players
            .replication(loading.loaded.binding.actor)
            .ok_or("entry sequence owner missing")?;
        let actor_state = crate::player_entry::prepare_player_entry_state(
            &saved,
            snapshot,
            loading.receipt.ok_or("entry login receipt missing")?,
            &replica.properties,
            setup,
        )?;
        let item_sequences = replica
            .item_properties
            .iter()
            .map(|(id, s)| (id.0, crate::player_entry::physics_sequences(s)))
            .collect();
        let items: Vec<_> = loading
            .loaded
            .inventory
            .iter()
            .map(|i| crate::player_entry::EntryInventoryItem {
                entity: &i.entity,
                placement: &i.placement,
            })
            .collect();
        let friends = bace_wire::FriendsUpdate {
            kind: bace_wire::FriendsUpdateKind::Full,
            friends: snapshot
                .entry_friends()
                .iter()
                .map(|f| bace_wire::FriendEntry {
                    object_id: f.character.0,
                    online: f.online,
                    name: f.name.clone(),
                })
                .collect(),
        };
        let character = loading
            .character_assets
            .as_ref()
            .ok_or("entry character assets missing")?;
        let assets = appearance.borrowed(character.char_gen());
        let spell_table = loading
            .spell_table
            .as_ref()
            .ok_or("entry spell definitions missing")?;
        let enchantments = snapshot
            .enchantments()
            .ok_or("entry registry missing")?
            .entries()
            .iter()
            .map(|entry| {
                crate::enchantment_saves::prepare_enchantment_projection(
                    entry,
                    crate::player_assets::player_enchantment_definition(
                        entry.spell,
                        spell_table,
                        &self.assets.spell_rows,
                    ),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let entry =
            crate::player_entry::prepare_player_entry(crate::player_entry::PlayerEntryInput {
                saved: &saved,
                items: &items,
                enchantments: &enchantments,
                friends,
                actor_state,
                item_sequences: &item_sequences,
                missile_combat: false,
                plussed: loading.loaded.is_plussed
                    || (self.assets.policy.override_character_permissions
                        && self.sessions[&key].account.access_level as u8 > 1),
                assets: &assets,
            })?;
        self.register_player_visibility(
            key,
            saved.player.entity.mutation_revision,
            snapshot.tick(),
            &entry,
        )?;
        self.players
            .queue_prepared_entry(key, &entry, entry_limits(self.limits.message_bytes))?;
        // This phase means queued, not entered; the exact trusted receipt still
        // gates UI and network world-state acceptance.
        self.sessions
            .get_mut(&key)
            .expect("session")
            .loading
            .as_mut()
            .expect("loading")
            .phase = lifecycle::Phase::QueuedEntry;
        Ok(())
    }
}
fn entry_limits(bytes: usize) -> bace_replication::LoginProjectionLimits {
    bace_replication::LoginProjectionLimits {
        batch: bace_replication::BatchLimits {
            max_messages: 4096,
            max_bytes: bytes,
            max_message_bytes: bytes,
            max_string_bytes: 4096,
        },
        description: bace_wire::PlayerDescriptionLimits {
            max_table_entries: 4096,
            max_string_bytes: 4096,
            max_gameplay_options_bytes: 1024 * 1024,
            max_message_bytes: bytes,
        },
        objects: bace_wire::ObjectCodecLimits {
            max_message_bytes: bytes,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 4096,
        },
        social: bace_wire::SocialCodecLimits {
            max_message_bytes: bytes,
            max_entries: 1024,
            max_filters: 1024,
            max_string_bytes: 4096,
        },
        max_titles: 1024,
        max_container_items: 1024,
    }
}
