pub fn install_tables() {
    if bace_loot::ace_tables::active_id() == Some(1) {
        return;
    }
    let source = include_str!("../../data/ace-treasure-tables.toml");
    let value: bace_content::TreasureTableSetV1 = toml::from_str(source).unwrap();
    if let Err(error) = bace_loot::ace_tables::install(value) {
        assert_eq!(bace_loot::ace_tables::active_id(), Some(1), "{error}");
    }
}
