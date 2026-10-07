//! Pure preparation of decoded character DAT tables. Blocking archive reads,
//! fingerprint admission and load_verified version checks belong to the asset
//! owner BEFORE this conversion; no file is reopened and no version is guessed.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use bace_character::{
    CreationRules, CreationRulesError, CreationSkillCosts, ProgressionTables, RankTable,
    RankTableError, SkillCosts, SkillRulesError, SkillTrainingRules,
};
use bace_dat::{CharGen, SkillTable, XpTable};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CharacterAssetError {
    #[error("invalid {table} progression table: {reason:?}")]
    RankTable {
        table: &'static str,
        reason: RankTableError,
    },
    #[error("invalid character-level XP/credit table shape")]
    CharacterLevels,
    #[error("character-generation heritage capacity exceeded")]
    HeritageCapacity,
    #[error("skill {skill} cannot be represented by pinned 55-entry creation allocation")]
    UnsupportedSkill { skill: u32 },
    #[error("skill {skill} has an invalid or overflowing specialization cost")]
    SkillCost { skill: u32 },
    #[error("invalid prepared skill training rules: {0:?}")]
    TrainingRules(SkillRulesError),
    #[error("heritage {heritage} repeats skill override {skill}")]
    DuplicateOverride { heritage: u32, skill: u32 },
    #[error("heritage {heritage} overrides absent skill {skill}")]
    UnknownOverride { heritage: u32, skill: u32 },
    #[error("heritage {heritage} has negative costs for skill {skill}")]
    OverrideCost { heritage: u32, skill: u32 },
    #[error("invalid creation rules for heritage {heritage}: {reason:?}")]
    CreationRules {
        heritage: u32,
        reason: CreationRulesError,
    },
}

/// One immutable prepared generation retaining every decoded field. Unconsumed
/// appearance/formula/template/level data remains available for later owners;
/// deriving allocation rules never discards it or claims those systems playable.
pub struct PreparedCharacterAssets {
    xp: XpTable,
    skills: SkillTable,
    chargen: CharGen,
    progression: Arc<ProgressionTables>,
    training: Arc<SkillTrainingRules>,
    creation: BTreeMap<u32, Arc<CreationRules>>,
}
impl PreparedCharacterAssets {
    pub fn xp_table(&self) -> &XpTable {
        &self.xp
    }
    pub fn skill_table(&self) -> &SkillTable {
        &self.skills
    }
    pub fn char_gen(&self) -> &CharGen {
        &self.chargen
    }
    pub fn progression(&self) -> Arc<ProgressionTables> {
        self.progression.clone()
    }
    pub fn training(&self) -> Arc<SkillTrainingRules> {
        self.training.clone()
    }
    /// These are allocation rules, not authorization to create Olthoi or bypass
    /// appearance/template/starting-area checks in the character lifecycle.
    pub fn creation(&self, heritage: u32) -> Option<Arc<CreationRules>> {
        self.creation.get(&heritage).cloned()
    }
}

/// Translate prepared metadata atomically. Specialization's base DAT field is
/// TOTAL cost, so its incremental cost is total minus training. Heritage primary
/// overrides are already incremental in ACE PlayerFactory and are copied as-is.
/// No content fallback, hard-coded price/rank table or partial result is returned.
pub fn prepare_character_assets(
    xp: XpTable,
    skills: SkillTable,
    chargen: CharGen,
) -> Result<PreparedCharacterAssets, CharacterAssetError> {
    let rank = |name, values: &[u32]| {
        RankTable::new(values).map_err(|reason| CharacterAssetError::RankTable {
            table: name,
            reason,
        })
    };
    let progression = Arc::new(ProgressionTables {
        attributes: rank("attributes", &xp.attribute_xp)?,
        vitals: rank("vitals", &xp.vital_xp)?,
        trained_skills: rank("trained skills", &xp.trained_skill_xp)?,
        specialized_skills: rank("specialized skills", &xp.specialized_skill_xp)?,
    });
    if xp.character_level_xp.is_empty()
        || xp.character_level_xp.len() > 65_536
        || xp.character_level_xp.len() != xp.character_level_skill_credits.len()
        || xp.character_level_xp[0] != 0
        || xp
            .character_level_xp
            .windows(2)
            .any(|pair| pair[0] > pair[1])
    {
        return Err(CharacterAssetError::CharacterLevels);
    }
    if chargen.heritage_groups.len() > 256 {
        return Err(CharacterAssetError::HeritageCapacity);
    }
    let mut base_creation = [None; 55];
    let mut costs = Vec::with_capacity(skills.skills.len().min(55));
    for (id, skill) in &skills.skills {
        if *id >= 55 {
            return Err(CharacterAssetError::UnsupportedSkill { skill: *id });
        }
        let specialized = skill
            .specialized_cost
            .checked_sub(skill.trained_cost)
            .filter(|value| *value >= 0 && skill.trained_cost >= 0)
            .ok_or(CharacterAssetError::SkillCost { skill: *id })?;
        costs.push(SkillCosts {
            skill: *id,
            trained_cost: skill.trained_cost,
            specialized_cost: specialized,
        });
        base_creation[*id as usize] = Some(CreationSkillCosts {
            trained: skill.trained_cost as u32,
            specialized: specialized as u32,
        });
    }
    let training =
        Arc::new(SkillTrainingRules::new(&costs).map_err(CharacterAssetError::TrainingRules)?);
    let mut creation = BTreeMap::new();
    for (id, heritage) in &chargen.heritage_groups {
        let mut resolved = base_creation;
        let mut overridden = BTreeSet::new();
        for entry in &heritage.skills {
            if !overridden.insert(entry.skill) {
                return Err(CharacterAssetError::DuplicateOverride {
                    heritage: *id,
                    skill: entry.skill,
                });
            }
            let slot = resolved
                .get_mut(entry.skill as usize)
                .and_then(Option::as_mut)
                .ok_or(CharacterAssetError::UnknownOverride {
                    heritage: *id,
                    skill: entry.skill,
                })?;
            if entry.normal_cost < 0 || entry.primary_cost < 0 {
                return Err(CharacterAssetError::OverrideCost {
                    heritage: *id,
                    skill: entry.skill,
                });
            }
            *slot = CreationSkillCosts {
                trained: entry.normal_cost as u32,
                specialized: entry.primary_cost as u32,
            };
        }
        let rules =
            CreationRules::new(heritage.attribute_credits, heritage.skill_credits, resolved)
                .map_err(|reason| CharacterAssetError::CreationRules {
                    heritage: *id,
                    reason,
                })?;
        creation.insert(*id, Arc::new(rules));
    }
    Ok(PreparedCharacterAssets {
        xp,
        skills,
        chargen,
        progression,
        training,
        creation,
    })
}
