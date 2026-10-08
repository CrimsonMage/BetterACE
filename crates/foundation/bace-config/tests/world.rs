use bace_config::{ServerConfig, WorldConfig};
fn config(extra: &str) -> String {
    format!(
        "{}\n{extra}",
        include_str!("../../../../tests/fixtures/config/server.toml")
    )
}
#[test]
fn omitted_world_uses_ace_defaults_but_explicit_empty_lists_replace_them() {
    let c = ServerConfig::parse(&config("")).unwrap();
    assert_eq!(c.world.zones.no_relog.len(), 37);
    assert_eq!(c.world.zones.no_death_item_drop.len(), 19);
    assert_eq!(c.world.zones.no_kill_experience.len(), 12);
    assert_eq!(c.world.preloading.entries.len(), 6);
    let hebian = &c.world.preloading.entries[0];
    assert_eq!(hebian.landblock, Some(0xe74e));
    assert!(hebian.enabled && hebian.permanent && !hebian.include_adjacent);
    let holtburg = &c.world.preloading.entries[1];
    assert_eq!(holtburg.landblock, Some(0xa9b4));
    assert!(!holtburg.enabled && holtburg.permanent && holtburg.include_adjacent);
    assert!(!c.world.death.creatures_drop_createlist_wield);
    let enabled = ServerConfig::parse(&config(
        "[world.death]\ncreatures_drop_createlist_wield=true",
    ))
    .unwrap();
    assert!(enabled.world.death.creatures_drop_createlist_wield);
    let c=ServerConfig::parse(&config("[world.zones]\nno_relog=[]\nno_death_item_drop=[0x005F]\nno_kill_experience=[]\n[world.preloading]\nentries=[]")).unwrap();
    assert!(c.world.zones.no_relog.is_empty());
    assert_eq!(c.world.zones.no_death_item_drop, vec![0x5f]);
    assert!(c.world.preloading.entries.is_empty());
}
#[test]
fn malformed_or_ambiguous_restart_policies_fail_closed() {
    for text in [
        "[world.zones]\nno_relog=[1,1]",
        "[world.zones]\nno_relog=[65536]",
        "[world.zones]\nno_corpse=[]",
        "[world.death]\nvitae_penalty=nan",
        "[world.death]\npk_server=true\npkl_server=true",
        "[[world.preloading.entries]]\nlandblock=1\napartment_group=true",
        "[[world.preloading.entries]]\ndescription='missing identity'",
    ] {
        assert!(ServerConfig::parse(&config(text)).is_err(), "{text}");
    }
    let mut c = WorldConfig::default();
    c.zones.no_relog = vec![0; 4097];
    assert!(c.validate().is_err());
}
