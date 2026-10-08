//! Synthetic packed hash-table order regression. Layout: pinned
//! ACE.DatLoader/FileTypes/ClothingTable.cs and Entity/CloSubPalEffect.cs.
#[test]
fn template_lookup_does_not_reorder_first_template_fallback() {
    let mut bytes = Vec::new();
    bytes.extend(0x10000001_u32.to_le_bytes());
    bytes.extend(0_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    for id in [94_u32, 8] {
        bytes.extend(id.to_le_bytes());
        bytes.extend(0_u32.to_le_bytes());
        bytes.extend(0_u32.to_le_bytes());
    }
    let table = bace_dat::ClothingTable::decode(&bytes).unwrap();
    assert_eq!(table.template_order, [94, 8]);
    assert_eq!(table.templates.keys().copied().collect::<Vec<_>>(), [8, 94]);
    for end in 0..bytes.len() {
        assert!(bace_dat::ClothingTable::decode(&bytes[..end]).is_err());
    }
}
