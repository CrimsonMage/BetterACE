use bace_content::SpellRowV1;
use bace_dat::{Animation, DatArchive, DualDidMapper, MotionTable, SpellComponents, SpellTable};
use bace_runtime::{
    magic_preparation::prepare_component_plan, native_magic_assets::*,
    world_admission::MotionChainRequest,
};
use std::collections::{BTreeMap, BTreeSet};
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn verified_dat_and_native_row_prepare_and_atomically_register_actual_cast_program() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut archive = DatArchive::open(path).unwrap();
    let table = MotionTable::decode(&archive.read(0x09000001).unwrap()).unwrap();
    let components =
        SpellComponents::decode(&archive.read(SpellComponents::RECORD_ID).unwrap()).unwrap();
    let spells = SpellTable::decode(&archive.read(SpellTable::RECORD_ID).unwrap()).unwrap();
    let mapper = DualDidMapper::decode(&archive.read(0x27000002).unwrap()).unwrap();
    let ids: BTreeSet<_> = table
        .links
        .values()
        .flat_map(|m| m.values())
        .chain(table.cycles.values())
        .flat_map(|d| d.animations.iter().map(|a| a.animation_id))
        .collect();
    let mut animations = BTreeMap::new();
    for id in ids {
        animations.insert(id, Animation::decode(&archive.read(id).unwrap()).unwrap());
    }
    let (&id, base) = spells
        .spells
        .iter()
        .find(|(_, s)| {
            s.meta_type == 3 && s.school == 2 && s.flags & 4 != 0 && !s.formula.is_empty()
        })
        .expect("real life boost");
    // Explicit native server-row fixture. This test qualifies DAT-to-owner glue,
    // never this authored amount as the stock world's damage value.
    let row:SpellRowV1=serde_json::from_value(serde_json::json!({"id":id,"name":base.name,"last_modified":"fixture","damage_type":128,"boost":10,"boost_variance":5})).unwrap();
    let plan = prepare_component_plan(base, &components, &mapper, b"native-test", [false; 5], &[])
        .unwrap();
    let asset = prepare_native_magic_spell(NativeMagicPreparation {
        id,
        base,
        row: &row,
        components: &components,
        component_plan: &plan,
        motion_table: &table,
        animations: &animations,
        motion_context: MotionChainRequest {
            style: 0x80000049,
            current_motion: 0x41000003,
            current_speed: 1.,
            action: 0,
            action_speed: 2.,
            scale: 1.,
            modifiers: &[],
        },
        projectile: None,
    })
    .unwrap();
    assert!(!asset.definition.spell.gestures.is_empty());
    assert!(
        asset
            .definition
            .spell
            .gestures
            .iter()
            .all(|g| g.motion_chain.as_ref().is_some_and(|c| c.is_rootless()))
    );
    assert_eq!(
        asset.definition.formula_level,
        bace_magic::spell_formula_level(&base.formula)
    );
    assert_eq!(asset.definition.flags, base.flags);
    assert_eq!(asset.definition.target_mask, base.non_component_target_type);
    assert_eq!(asset.definition.spell.components, plan.requirements);
    let mut kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let duplicate = asset.clone();
    assert!(register_native_magic_spells(&mut kernel, vec![asset.clone(), duplicate]).is_err());
    register_native_magic_spells(&mut kernel, vec![asset]).unwrap();
}
