pub fn install_tables() {
    if bace_loot::ace_tables::active_id() == Some(1) {
        return;
    }
    let source = include_str!("../../../../gameplay/bace-loot/data/ace-treasure-tables.toml");
    let value = bace_content_tools::parse_treasure_table_set(source).unwrap();
    if let Err(error) = bace_loot::ace_tables::install(value) {
        assert_eq!(bace_loot::ace_tables::active_id(), Some(1), "{error}");
    }
}
