//! Prepared generator overload-ID dispatch. Pinned ACE WorldObject_Generator
//! resolves TreasureDeath before TreasureWielded; selected death failures never
//! fall through to wielded content. Preparation belongs outside the owner tick.
use bace_content::{TreasureDeathRowV1, TreasureWieldedRowV1, WeenieV1};
use bace_loot::{DeathTreasure, TreasureAssets, TreasureError, TreasureRandom, WieldedTreasure};
use std::sync::Arc;

pub enum PreparedGeneratorTreasure {
    Death(Arc<DeathTreasure>),
    Wielded(WieldedTreasure),
}
impl PreparedGeneratorTreasure {
    pub fn treasure_type(&self) -> Option<u32> {
        match self {
            Self::Death(value) => Some(value.treasure_type()),
            Self::Wielded(value) => value.treasure_type(),
        }
    }

    pub fn prepare(
        overloaded_id: u32,
        death: Option<TreasureDeathRowV1>,
        wielded: Vec<TreasureWieldedRowV1>,
        assets: Arc<TreasureAssets>,
        aetheria_drop_rate: f32,
    ) -> Result<Self, TreasureError> {
        if overloaded_id == 0 {
            return Err(TreasureError::Bounds);
        }
        if let Some(death) = death {
            if death.treasure_type != overloaded_id {
                return Err(TreasureError::Bounds);
            }
            return Ok(Self::Death(Arc::new(DeathTreasure::prepare(
                death,
                assets,
                aetheria_drop_rate,
            )?)));
        }
        if wielded.is_empty() {
            return Err(TreasureError::MissingTable(overloaded_id));
        }
        if wielded.iter().any(|r| r.treasure_type != overloaded_id) {
            return Err(TreasureError::Bounds);
        }
        Ok(Self::Wielded(WieldedTreasure::prepare(wielded, |id| {
            assets.templates.get(&id).cloned()
        })?))
    }
    pub fn generate<R: TreasureRandom>(
        &self,
        random: &mut R,
    ) -> Result<Vec<WeenieV1>, TreasureError> {
        match self {
            Self::Death(v) => v.generate(random),
            Self::Wielded(v) => v.generate(random),
        }
    }
}
