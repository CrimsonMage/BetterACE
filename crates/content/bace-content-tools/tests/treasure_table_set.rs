use bace_content_tools::{
    compile_treasure_table_set, decode_treasure_table_set, parse_treasure_table_set,
};

#[test]
fn pinned_ace_tables_survive_frozen_pack_codec_with_order_and_bits() {
    let source = include_str!("../../../gameplay/bace-loot/data/ace-treasure-tables.toml");
    let tables = parse_treasure_table_set(source).unwrap();
    assert_eq!(tables.id, 1);
    assert_eq!(tables.tables.len(), 1264);
    assert_eq!(tables.scripts.len(), 51);
    let bytes = compile_treasure_table_set(&tables).unwrap();
    assert_eq!(decode_treasure_table_set(&bytes).unwrap(), tables);
    let mut bad_reference = tables.clone();
    bad_reference
        .tables
        .iter_mut()
        .find(|t| !t.references.is_empty())
        .unwrap()
        .references[0] = "Absent.Table".into();
    assert!(compile_treasure_table_set(&bad_reference).is_err());
    let mut cycle = tables.clone();
    let table = cycle
        .tables
        .iter_mut()
        .find(|t| !t.references.is_empty())
        .unwrap();
    table.references[0] = format!("{}.{}", table.class, table.field);
    assert!(compile_treasure_table_set(&cycle).is_err());
    let mut broken = bytes;
    broken[60] ^= 1;
    assert!(decode_treasure_table_set(&broken).is_err());
}
