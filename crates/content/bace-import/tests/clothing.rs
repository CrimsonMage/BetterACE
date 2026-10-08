//! Independent synthetic expectations from CustomClothingBase's documented JSON
//! and key-level merge contract (122145d0f6d0c159183f0f229aae611085a67bf1).
use bace_content::{PaletteSource, resolve_clothing};
use bace_import::{export_clothing_json, import_clothing_json};
const PATCH: &str = r#"{
  "Id":"0x1000fffe",
  "ClothingBaseEffects":{
    "0x02000001":{"CloObjectEffects":[
      {"Index":"9","ModelId":"0x01000010","CloTextureEffects":[{"OldTexture":"0x05000001","NewTexture":"0x05000002"},{"OldTexture":"0x05000003","NewTexture":"0x05000004"}]},
      {"Index":16,"ModelId":"0x01000011","CloTextureEffects":[]}
    ]},
    "33554446":{"CloObjectEffects":[{"Index":30,"ModelId":"0x01000012"}]}
  },
  "ClothingSubPalEffects":{
    "94":{"Icon":"0x06000001","CloSubPalettes":[
      {"PaletteSet":"0x04000001","Ranges":[{"Offset":0,"NumColors":8},{"Offset":24,"NumColors":16}]},
      {"PaletteSet":"0x0f000002","Ranges":[{"Offset":40,"NumColors":8}]}
    ]},
    "8":{"CloSubPalettes":[]}
  }
}"#;
#[test]
fn all_setups_parts_textures_and_palette_ranges_survive_import_and_export() {
    let p = import_clothing_json(PATCH).unwrap();
    assert_eq!(p.id, 0x1000fffe);
    assert_eq!(p.setups.len(), 2);
    assert_eq!(
        p.setups[0]
            .parts
            .iter()
            .map(|p| p.index)
            .collect::<Vec<_>>(),
        [9, 16]
    );
    assert_eq!(p.setups[0].parts[0].textures.len(), 2);
    assert_eq!(p.setups[0].parts[0].textures[1].new, 0x05000004);
    assert_eq!(p.setups[1].id, 0x0200000e);
    assert_eq!(p.setups[1].parts[0].index, 30);
    assert_eq!(
        p.palettes.iter().map(|p| p.template).collect::<Vec<_>>(),
        [94, 8]
    );
    assert_eq!(
        p.palettes[0].effects[0].source,
        PaletteSource::Palette(0x04000001)
    );
    assert_eq!(
        p.palettes[0].effects[1].source,
        PaletteSource::PaletteSet(0x0f000002)
    );
    assert_eq!(p.palettes[0].effects[0].ranges[1].colors, 16);
    let json = export_clothing_json(&p).unwrap();
    assert!(json.find("\"94\"").unwrap() < json.find("\"8\"").unwrap());
    assert_eq!(import_clothing_json(&json).unwrap(), p);
    assert_eq!(p.palette(None).unwrap().template, 94);
}
#[test]
fn merge_replaces_whole_matching_entries_and_keeps_other_variants() {
    let base = import_clothing_json(PATCH).unwrap();
    let update=import_clothing_json(r#"{"Id":"0x1000fffe","ClothingBaseEffects":{"0x02000001":{"CloObjectEffects":[{"Index":12,"ModelId":"0x01000020"}]}},"ClothingSubPalEffects":{"94":{"CloSubPalettes":[]},"100":{"CloSubPalettes":[]}}}"#).unwrap();
    let merged = resolve_clothing(Some(&base), &update).unwrap();
    assert_eq!(merged.setups[0].parts.len(), 1);
    assert_eq!(merged.setups[0].parts[0].index, 12);
    assert_eq!(merged.setups[1], base.setups[1]);
    assert!(merged.palettes[0].effects.is_empty());
    assert_eq!(
        merged
            .palettes
            .iter()
            .map(|p| p.template)
            .collect::<Vec<_>>(),
        [94, 8, 100]
    );
    assert_eq!(base.setups[0].parts.len(), 2);
    assert_eq!(resolve_clothing(None, &update).unwrap(), update);
    // Removing an override entry restores the original base, never a previous overlay.
    let empty = import_clothing_json(r#"{"Id":"0x1000fffe"}"#).unwrap();
    assert_eq!(resolve_clothing(Some(&base), &empty).unwrap(), base);
}
#[test]
fn bounds_duplicates_and_unknown_fields_reject_the_whole_patch() {
    for json in [
        r#"{"Id":"0x10000001","id":"0x10000002"}"#,
        r#"{"Id":"0x10000001","ClothingBaseEffects":{"33554433":{},"0x02000001":{}}}"#,
        r#"{"Id":"0x10000001","ClothingSubPalEffects":{"8":{"Unexpected":7}}}"#,
        r#"{"Id":"0x10000001","ClothingBaseEffects":{"0x02000001":{"CloObjectEffects":[{"Index":256,"ModelId":"0x01000001"}]}}}"#,
        r#"{"Id":"0x10000001","ClothingSubPalEffects":{"8":{"CloSubPalettes":[{"PaletteSet":"0x04000001","Ranges":[{"Offset":7,"NumColors":8}]}]}}}"#,
        r#"{"Id":4294967296}"#,
    ] {
        assert!(import_clothing_json(json).is_err(), "{json}");
    }
    assert!(import_clothing_json(&" ".repeat(bace_import::MAX_CLOTHING_BYTES + 1)).is_err());
    let p = import_clothing_json(r#"{"id":"268435457","clothingbaseeffects":{},}"#).unwrap();
    assert_eq!(p.id, 0x10000001);
    assert!(p.validate_assets(|_| false).is_ok());
    let p = import_clothing_json(PATCH).unwrap();
    assert!(p.validate_assets(|id| id != 0x05000004).is_err());
}

#[test]
fn full_palette_length_is_wire_representable() {
    let mut p = import_clothing_json(PATCH).unwrap();
    let r = &mut p.palettes[0].effects[0].ranges[0];
    r.offset = 0;
    r.colors = 2048;
    assert!(p.validate().is_ok());
    assert_eq!(
        import_clothing_json(&export_clothing_json(&p).unwrap()).unwrap(),
        p
    );
    p.palettes[0].effects[0].ranges[0].colors = 2056;
    assert!(p.validate().is_err());
}
