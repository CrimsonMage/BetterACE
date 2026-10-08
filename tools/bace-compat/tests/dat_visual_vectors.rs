use bace_dat::{
    ClothingTable, DatPalette, DatPaletteSet, DatSurface, GraphicsObject, ModelSetup,
    decode_texture_list,
};
use serde_json::Value;
fn vector(name: &str) -> (Vec<u8>, Value) {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/dat_visual.json")).unwrap();
    assert_eq!(fixture["commit"], bace_compat::ACE_COMMIT);
    let vector = &fixture["vectors"][name];
    let bytes = vector["bytes"]
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
        .collect();
    (bytes, vector["decoded"].clone())
}
#[test]
fn palettes_and_clothing_match_unmodified_ace_decoders() {
    let (bytes, expected) = vector("palette");
    let palette = DatPalette::decode(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&palette.colors).unwrap(),
        expected["Colors"]
    );
    for end in 0..bytes.len() {
        assert!(DatPalette::decode(&bytes[..end]).is_err());
    }
    let (bytes, expected) = vector("palette_set");
    let set = DatPaletteSet::decode(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&set.palettes).unwrap(),
        expected["PaletteList"]
    );
    assert_eq!(set.at_shade(0.0), Some(0x04000001));
    assert_eq!(set.at_shade(1.0), Some(0x04000002));
    assert_eq!(set.at_shade(f64::NAN), None);
    let (bytes, expected) = vector("clothing");
    let clothing = ClothingTable::decode(&bytes).unwrap();
    assert_eq!(clothing.template_order, vec![2]);
    let part = &clothing.setups[&0x02000001][0];
    let oracle = &expected["ClothingBaseEffects"]["33554433"]["CloObjectEffects"][0];
    assert_eq!(part.model as u64, oracle["ModelId"].as_u64().unwrap());
    assert_eq!(part.index as u64, oracle["Index"].as_u64().unwrap());
    assert_eq!(part.textures, vec![(0x05000001, 0x05000002)]);
    let template = &clothing.templates[&2];
    assert_eq!(
        template.icon as u64,
        expected["ClothingSubPalEffects"]["2"]["Icon"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(template.palettes[0].ranges, vec![(8, 16)]);
    assert_eq!(template.palettes[0].palette_set, 0x0f000001);
    for end in 0..bytes.len() {
        assert!(ClothingTable::decode(&bytes[..end]).is_err());
    }
}
#[test]
fn geometry_placements_and_materials_match_unmodified_ace() {
    let (bytes, expected) = vector("setup");
    let setup = ModelSetup::decode(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&setup.parts).unwrap(),
        expected["Parts"]
    );
    assert_eq!(setup.scales, vec![[2.0, 3.0, 4.0]]);
    assert_eq!(setup.placements[&0][0].origin, [1.0, 2.0, 3.0]);
    assert_eq!(setup.placements[&0][0].rotation, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        u64::from(setup.default_animation),
        expected["DefaultAnimation"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(setup.default_motion),
        expected["DefaultMotionTable"].as_u64().unwrap()
    );
    for end in 0..bytes.len() {
        assert!(ModelSetup::decode(&bytes[..end]).is_err());
    }
    let (bytes, expected) = vector("gfx");
    let gfx = GraphicsObject::decode(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&gfx.surfaces).unwrap(),
        expected["Surfaces"]
    );
    assert_eq!(gfx.vertices.len(), 3);
    assert_eq!(gfx.vertices[&1].position, [1.0, 0.0, 0.0]);
    assert_eq!(gfx.vertices[&2].uvs, vec![[0.0, 1.0]]);
    let p = &gfx.polygons[&4];
    assert_eq!(
        serde_json::to_value(&p.vertices).unwrap(),
        expected["Polygons"]["4"]["VertexIds"]
    );
    assert_eq!(p.positive_surface, 0);
    assert_eq!(p.negative_surface, 0);
    assert_eq!(p.negative_uvs, p.positive_uvs);
    for end in 0..bytes.len() {
        assert!(GraphicsObject::decode(&bytes[..end]).is_err());
    }
    let (bytes, expected) = vector("surface");
    let surface = DatSurface::decode(&bytes).unwrap();
    assert_eq!(
        u64::from(surface.texture),
        expected["OrigTextureId"].as_u64().unwrap()
    );
    assert_eq!(surface.palette, 0x04000001);
    assert_eq!(surface.diffuse, 0.75);
    let (bytes, expected) = vector("textures");
    assert_eq!(
        serde_json::to_value(decode_texture_list(&bytes).unwrap()).unwrap(),
        expected["Textures"]
    );
}

#[test]
fn vital_formulas_match_unmodified_ace() {
    let (bytes, expected) = vector("vitals");
    let table = bace_dat::VitalTable::decode(&bytes).unwrap();
    for (actual, name) in [
        (table.health, "MaxHealth"),
        (table.stamina, "MaxStamina"),
        (table.mana, "MaxMana"),
    ] {
        let formula = &expected[name]["Formula"];
        for (value, field) in [
            (actual.w, "W"),
            (actual.x, "X"),
            (actual.y, "Y"),
            (actual.z, "Z"),
            (actual.attribute1, "Attr1"),
            (actual.attribute2, "Attr2"),
        ] {
            assert_eq!(
                u64::from(value),
                formula[field].as_u64().unwrap(),
                "{name}.{field}"
            );
        }
    }
    for end in 0..bytes.len() {
        assert!(bace_dat::VitalTable::decode(&bytes[..end]).is_err());
    }
}
