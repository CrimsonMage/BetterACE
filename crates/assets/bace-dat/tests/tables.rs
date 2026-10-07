use bace_dat::{CharGen, DatArchive, DatTableLimits, DatTableVersion, SkillTable, XpTable};

fn xp_bytes() -> Vec<u8> {
    let mut bytes = XpTable::RECORD_ID.to_le_bytes().to_vec();
    for _ in 0..5 {
        bytes.extend_from_slice(&0u32.to_le_bytes());
    }
    for _ in 0..4 {
        bytes.extend_from_slice(&0u32.to_le_bytes());
    }
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}
#[test]
fn xp_exact_lengths_negative_counts_overflow_and_wrong_ids_are_rejected() {
    let bytes = xp_bytes();
    assert_eq!(XpTable::decode(&bytes).unwrap().attribute_xp, [0]);
    for end in 0..bytes.len() {
        assert!(XpTable::decode(&bytes[..end]).is_err());
    }
    let mut bad = bytes.clone();
    bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(XpTable::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(XpTable::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad[0] ^= 1;
    assert!(XpTable::decode(&bad).is_err());
    let mut bad = bytes;
    bad.push(0);
    assert!(XpTable::decode(&bad).is_err());
}
#[test]
fn empty_packed_skill_table_and_empty_chargen_are_bounded() {
    let mut skill = SkillTable::RECORD_ID.to_le_bytes().to_vec();
    skill.extend_from_slice(&[0, 0, 255, 255]);
    let table = SkillTable::decode(&skill).unwrap();
    assert!(table.skills.is_empty());
    assert_eq!(table.bucket_size, 65535);
    skill[4] = 255;
    assert!(SkillTable::decode(&skill).is_err());
    let mut cg = CharGen::RECORD_ID.to_le_bytes().to_vec();
    cg.extend_from_slice(&[0; 4]);
    cg.extend_from_slice(&[0, 1, 0]);
    assert!(CharGen::decode(&cg).unwrap().heritage_groups.is_empty());
    for end in 0..cg.len() {
        assert!(CharGen::decode(&cg[..end]).is_err());
    }
    assert!(
        CharGen::decode_with_limits(
            &cg,
            DatTableLimits {
                max_record_bytes: 4,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
#[ignore = "requires user-supplied portal DAT; set BACE_DAT_DIRECTORY"]
fn supplied_portal_progression_and_character_creation_tables() {
    let root = std::path::PathBuf::from(
        std::env::var("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY required"),
    );
    let path = root.join("client_portal.dat");
    // Smoke validation reports identity; it is not production fingerprint admission.
    eprintln!("portal sha256={}", bace_dat::fingerprint(&path).unwrap());
    let mut archive = DatArchive::open(path).unwrap();
    let header = archive.header();
    let version = |archive: &DatArchive, id| DatTableVersion {
        engine_version: header.engine_version,
        game_version: header.game_version,
        record_iteration: archive.records()[&id].iteration,
    };
    let xp_version = version(&archive, XpTable::RECORD_ID);
    let xp = XpTable::load_verified(&mut archive, xp_version).unwrap();
    let skills_version = version(&archive, SkillTable::RECORD_ID);
    let skills = SkillTable::load_verified(&mut archive, skills_version).unwrap();
    let chargen_version = version(&archive, CharGen::RECORD_ID);
    let chargen = CharGen::load_verified(&mut archive, chargen_version).unwrap();
    eprintln!("versions XP={xp_version:?} skills={skills_version:?} chargen={chargen_version:?}");
    eprintln!(
        "negative skill costs={:?}",
        skills
            .skills
            .iter()
            .filter(|(_, s)| s.trained_cost < 0 || s.specialized_cost < 0)
            .map(|(id, s)| (*id, s.trained_cost, s.specialized_cost))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "heritage override negatives={:?}",
        chargen
            .heritage_groups
            .iter()
            .flat_map(|(id, h)| h
                .skills
                .iter()
                .filter(|s| s.normal_cost < 0 || s.primary_cost < 0)
                .map(move |s| (*id, s.skill, s.normal_cost, s.primary_cost)))
            .collect::<Vec<_>>()
    );
    assert!(xp.attribute_xp.len() > 1 && xp.character_level_xp.len() > 1);
    assert!(!skills.skills.is_empty() && !chargen.heritage_groups.is_empty());
    eprintln!(
        "XP attribute={} vital={} trained={} specialized={} levels={}, skills={}, heritages={}",
        xp.attribute_xp.len(),
        xp.vital_xp.len(),
        xp.trained_skill_xp.len(),
        xp.specialized_skill_xp.len(),
        xp.character_level_xp.len(),
        skills.skills.len(),
        chargen.heritage_groups.len()
    );
    for id in [
        XpTable::RECORD_ID,
        SkillTable::RECORD_ID,
        CharGen::RECORD_ID,
    ] {
        let raw = archive.read(id).unwrap();
        // Sample prefixes across the entire real record plus every byte of its
        // final 128 bytes; exhaustive synthetic fixtures run in bace-compat.
        for end in (0..raw.len())
            .step_by(128)
            .chain(raw.len().saturating_sub(128)..raw.len())
        {
            let result = match id {
                XpTable::RECORD_ID => XpTable::decode(&raw[..end]).map(|_| ()),
                SkillTable::RECORD_ID => SkillTable::decode(&raw[..end]).map(|_| ()),
                _ => CharGen::decode(&raw[..end]).map(|_| ()),
            };
            assert!(result.is_err(), "accepted truncated {id:08x} prefix {end}");
        }
    }
    assert!(
        XpTable::load_verified(
            &mut archive,
            DatTableVersion {
                game_version: header.game_version.wrapping_add(1),
                ..xp_version
            }
        )
        .is_err()
    );
    assert!(
        SkillTable::load_verified(
            &mut archive,
            DatTableVersion {
                record_iteration: skills_version.record_iteration.wrapping_add(1),
                ..skills_version
            }
        )
        .is_err()
    );
}

#[test]
fn verified_load_checks_dataset_archive_version_and_record_iteration() {
    use std::io::Write;
    let raw = xp_bytes();
    let mut image = vec![0; 4096];
    let put = |bytes: &mut [u8], offset: usize, value: u32| {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes())
    };
    for (offset, value) in [
        (0x140, 0x5442),
        (0x144, 1024),
        (0x148, 4096),
        (0x14c, 1),
        (0x160, 1024),
        (0x174, 110),
        (0x178, 7),
        (1024, 2048),
        (1028 + 248, 1),
        (1028 + 256, XpTable::RECORD_ID),
        (1028 + 260, 3072),
        (1028 + 264, raw.len() as u32),
        (1028 + 272, 3),
    ] {
        put(&mut image, offset, value);
    }
    image[3076..3076 + raw.len()].copy_from_slice(&raw);
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&image).unwrap();
    let mut archive = DatArchive::open(file.path()).unwrap();
    let expected = DatTableVersion {
        engine_version: 110,
        game_version: 7,
        record_iteration: 3,
    };
    assert!(XpTable::load_verified(&mut archive, expected).is_ok());
    for wrong in [
        DatTableVersion {
            engine_version: 111,
            ..expected
        },
        DatTableVersion {
            game_version: 8,
            ..expected
        },
        DatTableVersion {
            record_iteration: 4,
            ..expected
        },
    ] {
        assert!(XpTable::load_verified(&mut archive, wrong).is_err());
    }
    image[0x14c..0x150].copy_from_slice(&2u32.to_le_bytes());
    let mut other = tempfile::NamedTempFile::new().unwrap();
    other.write_all(&image).unwrap();
    let mut archive = DatArchive::open(other.path()).unwrap();
    assert!(XpTable::load_verified(&mut archive, expected).is_err());
}
