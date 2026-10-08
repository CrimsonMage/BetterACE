//! Frozen V2 distinguishes allegiance-only quest XP from no-share and all-share.
//! The complete frozen V1 payload is retained; its queue variants provide the
//! explicit migration defaults. V1 field layouts and decoder remain unchanged.
use crate::{CodecLimits, NpcWorkflowSaveV1, SaveCodecError, npc_effects_v1::NpcEffectV1};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcSharingPolicyV2 {
    None,
    Allegiance,
    All,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcExperienceSharingV2 {
    pub ticket: u64,
    pub policy: NpcSharingPolicyV2,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcWorkflowSaveV2 {
    pub previous: NpcWorkflowSaveV1,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub experience_sharing: Vec<NpcExperienceSharingV2>,
}
impl std::ops::Deref for NpcWorkflowSaveV2 {
    type Target = NpcWorkflowSaveV1;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl From<NpcWorkflowSaveV1> for NpcWorkflowSaveV2 {
    fn from(previous: NpcWorkflowSaveV1) -> Self {
        let experience_sharing = previous
            .effects
            .iter()
            .filter_map(|p| {
                let policy = match p.effect {
                    NpcEffectV1::QueuedExperience { .. } => NpcSharingPolicyV2::None,
                    NpcEffectV1::QueuedSharedExperience { .. } => NpcSharingPolicyV2::All,
                    _ => return None,
                };
                Some(NpcExperienceSharingV2 {
                    ticket: p.ticket,
                    policy,
                })
            })
            .collect();
        Self {
            previous,
            experience_sharing,
        }
    }
}
impl NpcWorkflowSaveV2 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        if self.experience_sharing.len() > 4096 {
            return Err(SaveCodecError::Invalid("NPC sharing capacity"));
        }
        let mut policies = std::collections::BTreeMap::new();
        for entry in &self.experience_sharing {
            if policies.insert(entry.ticket, entry.policy).is_some() {
                return Err(SaveCodecError::Invalid("duplicate NPC sharing ticket"));
            }
        }
        for effect in &self.previous.effects {
            let required = match effect.effect {
                NpcEffectV1::QueuedExperience { .. } => Some(false),
                NpcEffectV1::QueuedSharedExperience { .. } => Some(true),
                _ => None,
            };
            if let Some(all) = required {
                let policy = policies
                    .remove(&effect.ticket)
                    .ok_or(SaveCodecError::Invalid("missing NPC sharing policy"))?;
                if (policy == NpcSharingPolicyV2::All) != all {
                    return Err(SaveCodecError::Invalid("inconsistent NPC sharing policy"));
                }
            }
        }
        if !policies.is_empty() {
            return Err(SaveCodecError::Invalid("unbound NPC sharing policy"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(120, 2, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 120, 2, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let header = crate::inspect(bytes, limits())?;
        if header.kind != 120 {
            return Err(SaveCodecError::Invalid("NPC workflow kind"));
        }
        match header.schema_version {
            1 => {
                let value: Self = NpcWorkflowSaveV1::decode(bytes)?.into();
                value.validate()?;
                Ok(value)
            }
            2 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported NPC workflow schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
