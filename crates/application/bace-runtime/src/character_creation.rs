//! Translate a character-domain proposal into frozen database DTOs. This is
//! storage composition only; appearance, allocation and gear policy stay in the
//! character owner. No success is reported until GameLoginService commits it.
use bace_character::PreparedCharacter;
use bace_storage_codec::{CharacterMetadataV1, EntitySaveV1, PlayerSaveV1, SaveCodecError};

pub struct FrozenCharacterCreation {
    pub player: PlayerSaveV1,
    pub items: Vec<(EntitySaveV1, u32)>,
}

pub fn freeze_creation(
    prepared: PreparedCharacter,
) -> Result<FrozenCharacterCreation, SaveCodecError> {
    if prepared.possessions.len() > 1023 {
        return Err(SaveCodecError::Invalid("starting inventory count"));
    }
    let metadata = CharacterMetadataV1 {
        hair_texture: prepared.metadata.hair_texture,
        default_hair_texture: prepared.metadata.default_hair_texture,
        options1: prepared.metadata.options1,
        options2: prepared.metadata.options2,
        titles: prepared.metadata.titles,
        // Supported humanoid factory has no initial spell-bar entries. The
        // unsupported Olthoi factory must supply its own explicit projection.
        spell_favorites: Vec::new(),
    };
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: prepared.entity,
            template_revision: prepared.template_revision,
            mutation_revision: 1,
            state: prepared.state,
        },
        account_id: prepared.account,
        name: prepared.name,
        metadata,
        quests: Vec::new(),
    };
    player.validate()?;
    let mut items = Vec::with_capacity(prepared.possessions.len());
    for (slot, item) in prepared.possessions.into_iter().enumerate() {
        let item = EntitySaveV1 {
            object_id: item.entity,
            template_revision: item.template_revision,
            mutation_revision: 1,
            state: item.state,
        };
        item.validate()?;
        items.push((item, slot as u32));
    }
    Ok(FrozenCharacterCreation { player, items })
}

impl FrozenCharacterCreation {
    pub fn login_input(&self, slot: u16) -> crate::game_login::PreparedCharacter<'_> {
        crate::game_login::PreparedCharacter {
            player: &self.player,
            slot,
            items: &self.items,
        }
    }
}
