use bace_runtime::{region_activation::RegionAssetManifest, server_magic_assets::*};
use bace_storage_codec::{PackKey, PackLookup};
use std::{path::PathBuf, sync::Arc};
#[test]
#[ignore = "requires approved DATs and accepted full native pack"]
fn actual_server_spell_has_no_account_formula_and_uses_real_cast_motion() {
    let dat = PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let manifest = RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let path = PathBuf::from(std::env::var_os("BACE_CREATION_PACK_MANIFEST").unwrap());
    let pack = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = Arc::new(
        pack.open(path.parent().unwrap(), Default::default())
            .unwrap(),
    );
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: 1,
        })
        .unwrap()
    else {
        panic!("human source")
    };
    let source: Arc<bace_content::WeenieV1> =
        Arc::new(bace_content_tools::decode(record.bytes()).unwrap());
    let mut portal = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let spells =
        bace_dat::SpellTable::decode(&portal.read(bace_dat::SpellTable::RECORD_ID).unwrap())
            .unwrap();
    let (&spell, _) = spells
        .spells
        .iter()
        .find(|(_, s)| s.school == 2 && s.meta_type == 3 && s.flags & 4 != 0)
        .unwrap();
    let mut assets = ServerMagicAssets::open(&manifest).unwrap();
    let mut work = ServerMagicWork {
        token: 1,
        actor: bace_types::EntityId(77),
        source_revision: 0,
        source: source.clone(),
        generation,
        spell,
        instant: true,
        motion: None,
    };
    let prepared = assets.prepare(&work).unwrap();
    assert!(prepared.spell.definition.spell.components.is_empty());
    assert!(prepared.spell.definition.spell.gestures.is_empty());
    assert_eq!(prepared.spell.definition.spell.spell.id, spell);
    for meta in [1, 2] {
        work.spell = spells
            .spells
            .iter()
            .find(|(_, s)| s.meta_type == meta && (meta != 2 || s.school == 1))
            .map(|(id, _)| *id)
            .unwrap();
        let prepared = assets.prepare(&work).unwrap();
        assert_eq!(prepared.spell.projectile.is_some(), meta == 2);
        assert!(matches!(
            prepared.spell.definition.spell.spell.effect,
            bace_magic::SpellEffect::Enchantment(_) | bace_magic::SpellEffect::Projectile(_)
        ));
    }
    work.spell = spell;
    let motion_id = source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 2)
        .unwrap()
        .value;
    let table = bace_dat::MotionTable::decode(&portal.read(motion_id).unwrap()).unwrap();
    work.instant = false;
    work.motion = Some(bace_motion::SourceMotionState {
        style: table.default_style,
        substate: 0x41000003,
        speed: 1.,
    });
    let prepared = assets.prepare(&work).unwrap();
    let gesture = &prepared.spell.definition.spell.gestures[0];
    assert_eq!(gesture.gesture.motion, 0x400000d3);
    assert_eq!(gesture.gesture.minimum_seconds, 2.);
    assert_eq!(gesture.motion_chain.as_ref().unwrap().speed, 2.);
    assert!(
        gesture
            .motion_chain
            .as_ref()
            .unwrap()
            .stop_chain()
            .is_some()
    );
}
