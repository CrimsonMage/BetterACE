//! Dormant exact-definition registration and canonical source recovery queue.
use super::*;
impl NpcCoordinator {
    pub fn recover(
        &mut self,
        recovery: crate::npc_recovery::PreparedNpcRecovery,
        use_radius: f32,
    ) -> Result<(), (String, Box<crate::npc_recovery::PreparedNpcRecovery>)> {
        self.recover_with_registry(recovery, use_radius, None, None)
    }
    pub(crate) fn recover_with_registry(
        &mut self,
        recovery: crate::npc_recovery::PreparedNpcRecovery,
        use_radius: f32,
        object_registry: Option<bace_simulation::PreparedNpcRegistryRestore>,
        admission: Option<bace_simulation::NpcScriptIdentity>,
    ) -> Result<(), (String, Box<crate::npc_recovery::PreparedNpcRecovery>)> {
        if object_registry.as_ref().is_some_and(|r| {
            r.entity.0 != recovery.binding.source || r.entries.len() > 512 || recovery.archived()
        }) {
            return Err((
                "NPC recovered object registry identity".into(),
                Box::new(recovery),
            ));
        }
        if !use_radius.is_finite() {
            return Err(("NPC recovery use radius".into(), Box::new(recovery)));
        }
        let cost = recovery.source.retained_bytes
            + recovery
                .checkpoint
                .archive
                .as_ref()
                .and_then(|a| a.retained_bytes())
                .unwrap_or(0)
            + recovery.checkpoint_bytes;
        if self
            .sources
            .values()
            .map(|s| s.retained_bytes)
            .sum::<usize>()
            .checked_add(cost)
            .is_none_or(|n| n > 64 * 1024 * 1024)
        {
            return Err((
                "NPC recovery retained byte budget".into(),
                Box::new(recovery),
            ));
        }
        let Some(next) = self
            .next_correlation
            .checked_add(if object_registry.is_some() { 4 } else { 3 })
        else {
            return Err(("NPC correlation overflow".into(), Box::new(recovery)));
        };
        let source = EntityId(recovery.binding.source);
        if let Err(error) = self.bind(recovery.binding, recovery.workflow_version) {
            return Err((error, Box::new(recovery)));
        }
        let mut actions = Vec::new();
        if recovery.archived() {
            actions.push(A::RegisterArchive {
                checkpoint: recovery.checkpoint,
                program: recovery.source.program,
                use_radius,
            });
        } else {
            if let Some(identity) = admission {
                actions.push(A::BindAdmitted {
                    actor: source,
                    identity,
                });
            } else {
                actions.push(A::RegisterRecovery {
                    actor: source,
                    program: recovery.source.program,
                    use_radius,
                    properties: Some(recovery.source.properties),
                });
            }
            actions.push(A::Restore {
                checkpoint: recovery.checkpoint,
            });
        }
        if let Some(registry) = object_registry {
            actions.push(A::RestoreObjectRegistry {
                source,
                revision: registry.revision,
                entries: registry.entries,
            });
        }
        actions.push(A::RecoveryReady { source });
        let state = self.sources.get_mut(&source).expect("new binding");
        state.recovery = actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| NpcServiceCommand {
                correlation: self.next_correlation + index as u64,
                action,
            })
            .collect();
        state.recovering = true;
        state.generation = Some(recovery.generation);
        state.retained_bytes = cost;
        self.next_correlation = next;
        Ok(())
    }
    pub(crate) fn register_completed_source(
        &mut self,
        registration: &crate::npc_sources::PreparedNpcRegistration,
        snapshot: bace_simulation::NpcSourceCheckpoint,
    ) -> Result<(), String> {
        let source = registration.actor;
        if self.busy(source) {
            return Err("NPC completed source registration busy".into());
        }
        let next = self
            .next_correlation
            .checked_add(if registration.object_registry.is_some() {
                4
            } else {
                3
            })
            .ok_or("NPC correlation overflow")?;
        let cost = snapshot
            .properties
            .as_ref()
            .map_or(Some(0), |p| p.retained_bytes())
            .and_then(|n| {
                n.checked_add(snapshot.source_quests.as_ref().map_or(0, |(_, rows)| {
                    rows.iter().map(|(name, _)| name.len() + 32).sum::<usize>()
                }))
            })
            .ok_or("NPC completed source byte overflow")?;
        let used = self
            .sources
            .values()
            .try_fold(cost, |n, s| n.checked_add(s.retained_bytes))
            .ok_or("NPC retained source byte overflow")?;
        if used > 64 * 1024 * 1024 {
            return Err("NPC completed source byte capacity".into());
        }
        let mut actions = vec![
            if registration.admitted {
                A::BindAdmitted {
                    actor: source,
                    identity: registration.script_identity(),
                }
            } else {
                A::RegisterRecovery {
                    actor: source,
                    program: registration.source.program.clone(),
                    use_radius: registration.use_radius,
                    properties: Some(registration.source.properties.clone()),
                }
            },
            A::RestoreIdleSourceState {
                checkpoint: snapshot,
            },
        ];
        if let Some(registry) = &registration.object_registry {
            actions.push(A::RestoreObjectRegistry {
                source,
                revision: registry.revision,
                entries: registry.entries.clone(),
            });
        }
        actions.push(A::RecoveryReady { source });
        let commands: VecDeque<_> = actions
            .into_iter()
            .enumerate()
            .map(|(n, action)| NpcServiceCommand {
                correlation: self.next_correlation + n as u64,
                action,
            })
            .collect();
        if commands.iter().any(|c| !c.valid_bounds()) {
            return Err("NPC completed source state bounds".into());
        }
        let state = self
            .sources
            .get_mut(&source)
            .ok_or("NPC completed source missing")?;
        state.recovery = commands;
        state.recovering = true;
        state.retained_bytes = cost;
        self.next_correlation = next;
        Ok(())
    }
    pub(crate) fn activate_admitted_source(
        &mut self,
        registration: &crate::npc_sources::PreparedNpcRegistration,
    ) -> Result<(), String> {
        let source = registration.actor;
        if self.busy(source) {
            return Err("NPC fresh binding busy".into());
        }
        let next = self
            .next_correlation
            .checked_add(2)
            .ok_or("NPC fresh correlation overflow")?;
        let state = self
            .sources
            .get_mut(&source)
            .ok_or("NPC fresh binding missing")?;
        state.recovery = [
            A::BindAdmitted {
                actor: source,
                identity: registration.script_identity(),
            },
            A::RecoveryReady { source },
        ]
        .into_iter()
        .enumerate()
        .map(|(i, action)| NpcServiceCommand {
            correlation: self.next_correlation + i as u64,
            action,
        })
        .collect();
        state.recovering = true;
        self.next_correlation = next;
        Ok(())
    }
}
