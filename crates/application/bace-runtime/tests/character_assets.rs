use std::collections::BTreeMap;

use bace_character::CharacterProgression;
use bace_dat::{
    CharGen, CreationSkill, HeritageGroup, SkillBase, SkillFormula, SkillTable, XpTable,
};
use bace_gameplay_api::{CreationAllocation, CreationAttributes, SkillAdvancement, TrainSkill};
use bace_runtime::character_assets::{CharacterAssetError, prepare_character_assets};

fn fixture() -> (XpTable, SkillTable, CharGen) {
    // Synthetic cumulative tables already exercised by bace-character's
    // independent official C# progression oracle, not invented runtime defaults.
    let xp = XpTable {
        attribute_xp: vec![0, 10, 30, 30, 100],
        vital_xp: vec![0, 2, 8, 20, 100],
        trained_skill_xp: vec![0, 5, 15, 50, 100],
        specialized_skill_xp: vec![0, 1, 7, 40, 100],
        character_level_xp: vec![0, 1000, 3000],
        character_level_skill_credits: vec![0, 1, 2],
    };
    let skill = |trained, total| SkillBase {
        description: "retained description".into(),
        name: "synthetic".into(),
        icon_id: 123,
        trained_cost: trained,
        specialized_cost: total,
        category: 1,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 1,
            x: 1,
            y: 0,
            z: 2,
            attribute1: 2,
            attribute2: 4,
        },
        upper_bound: 1.0,
        lower_bound: 0.0,
        learn_modifier: 0.03,
    };
    let skills = SkillTable {
        bucket_size: 17,
        skills: BTreeMap::from([(6, skill(6, 18)), (7, skill(4, 10))]),
    };
    let heritage = HeritageGroup {
        name: "synthetic heritage".into(),
        icon: 42,
        setup: 43,
        environment_setup: 44,
        attribute_credits: 330,
        skill_credits: 52,
        primary_start_areas: vec![],
        secondary_start_areas: vec![],
        skills: vec![CreationSkill {
            skill: 6,
            normal_cost: 2,
            primary_cost: 3,
        }],
        templates: vec![],
        gender_marker: 1,
        genders: BTreeMap::new(),
    };
    let chargen = CharGen {
        reserved: 77,
        starter_areas: vec![],
        heritage_marker: 1,
        heritage_groups: BTreeMap::from([(1, heritage)]),
    };
    (xp, skills, chargen)
}

#[test]
fn prepared_rules_use_real_inputs_and_keep_heritage_overrides_out_of_later_training() {
    let (xp, skills, chargen) = fixture();
    let prepared = prepare_character_assets(xp.clone(), skills.clone(), chargen.clone()).unwrap();
    assert_eq!(prepared.xp_table(), &xp);
    assert_eq!(prepared.skill_table(), &skills);
    assert_eq!(prepared.char_gen(), &chargen);
    assert!(std::sync::Arc::ptr_eq(
        &prepared.progression(),
        &prepared.progression()
    ));
    assert!(prepared.creation(999).is_none());
    let mut allocation = CreationAllocation {
        attributes: CreationAttributes {
            strength: 100,
            endurance: 100,
            coordination: 100,
            quickness: 10,
            focus: 10,
            self_attribute: 10,
        },
        skills: [SkillAdvancement::Inactive; 55],
    };
    allocation.skills[6] = SkillAdvancement::Specialized;
    allocation.skills[7] = SkillAdvancement::Trained;
    // PlayerFactory uses heritage normal+primary directly: 2+3+4 = 9.
    assert_eq!(
        prepared
            .creation(1)
            .unwrap()
            .validate(&allocation)
            .unwrap()
            .available_skill_credits,
        43
    );
    let mut player = CharacterProgression::new(&[], prepared.progression(), 0, 0)
        .unwrap()
        .with_training(prepared.training(), 100, &[])
        .unwrap();
    assert!(
        player
            .train_skill(TrainSkill {
                skill: 6,
                quoted_credits: 2
            })
            .is_err()
    );
    assert_eq!(
        player
            .train_skill(TrainSkill {
                skill: 6,
                quoted_credits: 6
            })
            .unwrap()
            .available_skill_credits,
        94
    );
    // SkillBase.UpgradeCostFromTrainedToSpecialized = 18 - 6, not 18 or 3.
    assert_eq!(
        player.specialize_skill(6).unwrap().available_skill_credits,
        82
    );
}

#[test]
fn invalid_or_unrepresentable_skill_costs_fail_the_entire_preparation() {
    for (trained_cost, specialized_cost) in [(-1, 10), (6, 5), (i32::MIN, i32::MAX)] {
        let (xp, mut skills, chargen) = fixture();
        let skill = skills.skills.get_mut(&6).unwrap();
        skill.trained_cost = trained_cost;
        skill.specialized_cost = specialized_cost;
        assert!(matches!(
            prepare_character_assets(xp, skills, chargen),
            Err(CharacterAssetError::SkillCost { skill: 6 })
        ));
    }
    let (xp, mut skills, chargen) = fixture();
    let skill = skills.skills[&6].clone();
    skills.skills.insert(55, skill);
    assert!(matches!(
        prepare_character_assets(xp, skills, chargen),
        Err(CharacterAssetError::UnsupportedSkill { skill: 55 })
    ));
}

#[test]
fn malformed_heritage_overrides_do_not_produce_partial_rules() {
    let (xp, skills, mut chargen) = fixture();
    let overrides = &mut chargen.heritage_groups.get_mut(&1).unwrap().skills;
    overrides.push(overrides[0]);
    assert!(matches!(
        prepare_character_assets(xp, skills, chargen),
        Err(CharacterAssetError::DuplicateOverride {
            heritage: 1,
            skill: 6
        })
    ));
    let (xp, skills, mut chargen) = fixture();
    chargen.heritage_groups.get_mut(&1).unwrap().skills[0].skill = 54;
    assert!(matches!(
        prepare_character_assets(xp, skills, chargen),
        Err(CharacterAssetError::UnknownOverride {
            heritage: 1,
            skill: 54
        })
    ));
    let (xp, skills, mut chargen) = fixture();
    chargen.heritage_groups.get_mut(&1).unwrap().skills[0].primary_cost = -1;
    assert!(matches!(
        prepare_character_assets(xp, skills, chargen),
        Err(CharacterAssetError::OverrideCost {
            heritage: 1,
            skill: 6
        })
    ));
}

#[test]
fn malformed_progression_and_level_tables_are_not_substituted() {
    let (mut xp, skills, chargen) = fixture();
    xp.attribute_xp = vec![0, 100, 99];
    assert!(matches!(
        prepare_character_assets(xp, skills, chargen),
        Err(CharacterAssetError::RankTable {
            table: "attributes",
            ..
        })
    ));
    for mode in 0..3 {
        let (mut xp, skills, chargen) = fixture();
        match mode {
            0 => {
                xp.character_level_skill_credits.pop();
            }
            1 => xp.character_level_xp[0] = 1,
            _ => xp.character_level_xp[2] = 999,
        }
        assert!(matches!(
            prepare_character_assets(xp, skills, chargen),
            Err(CharacterAssetError::CharacterLevels)
        ));
    }
}

#[test]
#[ignore = "requires user-supplied portal DAT at BACE_DAT_DIRECTORY, fingerprint pinned in docs/baselines.toml"]
fn supplied_portal_tables_prepare_without_fabricated_defaults() {
    let directory = std::path::PathBuf::from(
        std::env::var("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY required"),
    );
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut archive = bace_dat::DatArchive::open(path).unwrap();
    // This is a supplied-asset integration witness, not a production admission
    // manifest: report observed versions and decode the three exact record IDs.
    let header = archive.header();
    eprintln!(
        "portal observed engine={} game={}",
        header.engine_version, header.game_version
    );
    assert_eq!(header.dataset, 1);
    let xp = XpTable::decode(&archive.read(XpTable::RECORD_ID).unwrap()).unwrap();
    let skills = SkillTable::decode(&archive.read(SkillTable::RECORD_ID).unwrap()).unwrap();
    let chargen = CharGen::decode(&archive.read(CharGen::RECORD_ID).unwrap()).unwrap();
    let count = chargen.heritage_groups.len();
    let prepared = prepare_character_assets(xp, skills, chargen).unwrap();
    assert!(prepared.progression().attributes.maximum_rank() > 0);
    for id in prepared.char_gen().heritage_groups.keys() {
        assert!(prepared.creation(*id).is_some());
    }
    eprintln!(
        "prepared heritages={count}; skills={}",
        prepared.skill_table().skills.len()
    );
}
