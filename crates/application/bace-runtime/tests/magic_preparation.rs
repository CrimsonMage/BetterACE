//! Explicit local-asset qualification; no proprietary records are embedded.
use bace_dat::{Animation, DatArchive, MotionTable, SpellComponents, SpellTable};
use bace_runtime::{
    magic_preparation::prepare_cast_gestures_from_assets, world_admission::MotionChainRequest,
};
use std::collections::BTreeMap;
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_spell_formulas_resolve_sequential_human_cast_chains() {
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = DatArchive::open(path).unwrap();
    let table = MotionTable::decode(&dat.read(0x09000001).unwrap()).unwrap();
    let components =
        SpellComponents::decode(&dat.read(SpellComponents::RECORD_ID).unwrap()).unwrap();
    let spells = SpellTable::decode(&dat.read(SpellTable::RECORD_ID).unwrap()).unwrap();
    let ids: std::collections::BTreeSet<_> = table
        .links
        .values()
        .flat_map(|m| m.values())
        .chain(table.cycles.values())
        .flat_map(|d| d.animations.iter().map(|a| a.animation_id))
        .collect();
    let mut animations = BTreeMap::new();
    for id in ids {
        animations.insert(id, Animation::decode(&dat.read(id).unwrap()).unwrap());
    }
    let mut formulas = 0;
    let mut gestures = 0;
    let mut failures = Vec::new();
    for (id, base) in &spells.spells {
        if !(1..=5).contains(&base.school) || base.formula.is_empty() {
            continue;
        }
        match prepare_cast_gestures_from_assets(
            base,
            &components,
            &table,
            &animations,
            MotionChainRequest {
                style: 0x80000049,
                current_motion: 0x41000003,
                current_speed: 1.0,
                action: 0,
                action_speed: 2.0,
                scale: 1.0,
                modifiers: &[],
            },
        ) {
            Ok(chains) => {
                formulas += 1;
                gestures += chains.len();
                for gesture in chains {
                    assert!(
                        gesture.motion_chain.unwrap().is_rootless(),
                        "spell {id} has unqualified root motion"
                    );
                }
            }
            Err(error) => {
                let mut current = 0x41000003;
                let mut speed = 1.0;
                let _ = bace_runtime::magic_preparation::prepare_cast_gestures(
                    base,
                    &components,
                    |action, action_speed| {
                        let result = bace_runtime::world_admission::prepare_motion_chain(
                            &table,
                            &animations,
                            MotionChainRequest {
                                style: 0x80000049,
                                current_motion: current,
                                current_speed: speed,
                                action,
                                action_speed,
                                scale: 1.0,
                                modifiers: &[],
                            },
                        );
                        if let Err(e) = &result {
                            eprintln!("spell {id} from{current:08x} to{action:08x}: {e}");
                        }
                        if action & 0x10000000 == 0 && action & 0x40000000 != 0 {
                            current = action;
                            speed = action_speed;
                        }
                        result.ok()
                    },
                );
                failures.push((*id, error));
            }
        }
    }
    eprintln!(
        "qualified {formulas} spell formulas / {gestures} sequential gestures; failures={}",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "sequential preparation refused: {failures:?}"
    );
    assert!(formulas > 1000);
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn native_item_and_creature_school_ids_select_distinct_top_level_foci() {
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = DatArchive::open(path).unwrap();
    let spells = SpellTable::decode(&dat.read(SpellTable::RECORD_ID).unwrap()).unwrap();
    let components =
        SpellComponents::decode(&dat.read(SpellComponents::RECORD_ID).unwrap()).unwrap();
    let mapper = bace_dat::DualDidMapper::decode(
        &dat.read(bace_dat::DualDidMapper::COMPONENT_RECORD_ID)
            .unwrap(),
    )
    .unwrap();
    for (school, foci, slot) in [(3, 15269, 3), (4, 15268, 2)] {
        let base = spells
            .spells
            .values()
            .find(|spell| {
                spell.school == school
                    && bace_magic::foci_formula(&spell.formula)
                        .ok()
                        .zip(spell.formula_for_account(b"fixture").ok())
                        .is_some_and(|(a, b)| a != b)
            })
            .unwrap();
        let expected = bace_magic::foci_formula(&base.formula).unwrap();
        let correct = bace_runtime::magic_preparation::prepare_component_plan(
            base,
            &components,
            &mapper,
            b"fixture",
            [false; 5],
            &[foci],
        )
        .unwrap();
        assert_eq!(correct.component_ids, expected);
        let wrong = bace_runtime::magic_preparation::prepare_component_plan(
            base,
            &components,
            &mapper,
            b"fixture",
            [false; 5],
            &[if foci == 15269 { 15268 } else { 15269 }],
        )
        .unwrap();
        assert_ne!(wrong.component_ids, expected);
        let mut infused = [false; 5];
        infused[slot] = true;
        let augmented = bace_runtime::magic_preparation::prepare_component_plan(
            base,
            &components,
            &mapper,
            b"fixture",
            infused,
            &[],
        )
        .unwrap();
        assert_eq!(augmented.component_ids, expected);
    }
}
