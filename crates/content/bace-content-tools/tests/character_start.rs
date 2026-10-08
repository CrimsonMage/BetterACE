use bace_content_tools::{compile_character_start, decode_character_start, parse_character_start};
#[test]
fn pinned_native_profile_matches_frozen_runtime_bytes_and_preserves_source_rows() {
    let profile = parse_character_start(include_str!(
        "../../../application/bace-runtime/data/character-start.toml"
    ))
    .unwrap();
    assert_eq!(profile.gear.len(), 107);
    assert_eq!(profile.spells.len(), 33);
    assert_eq!(profile.starts.len(), 5);
    // Explicit corrected source aliases, formerly weenieID instead of weenieId.
    assert!(profile.gear.iter().any(|g| g.template == 45542));
    assert!(profile.gear.iter().any(|g| g.template == 45546));
    assert_eq!(
        compile_character_start(&profile).unwrap(),
        include_bytes!("../../../application/bace-runtime/data/character-start.bace")
    );
}
#[test]
fn profile_integrity_version_and_invalid_references_are_rejected() {
    let mut bytes =
        include_bytes!("../../../application/bace-runtime/data/character-start.bace").to_vec();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert!(decode_character_start(&bytes).is_err());
    let mut profile = parse_character_start(include_str!(
        "../../../application/bace-runtime/data/character-start.toml"
    ))
    .unwrap();
    profile.schema_version = 2;
    assert!(compile_character_start(&profile).is_err());
    profile.schema_version = 1;
    profile.gear[0].template = 0;
    assert!(compile_character_start(&profile).is_err());
    assert!(
        parse_character_start(
            "schema_version=1\nhuman_template=1\ndefault_start_spell=3815\nunknown=3"
        )
        .is_err()
    );
}
