use super::requires_source;

fn bare(kind: u32) -> bace_content::WeenieV1 {
    bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 50,
        class_name: "source_registration_probe".into(),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}

#[test]
fn unscripted_shop_still_requires_canonical_npc_source() {
    // Pinned ACE WorldObjectFactory.CreateWorldObject switches directly on
    // WeenieType.Vendor. No emote or Bool79 is required to construct a Shop.
    assert!(requires_source(&bare(12)));
    assert!(!requires_source(&bare(1)));

    let mut marked = bare(1);
    marked.properties.bools.push(bace_content::Property {
        id: 79,
        value: true,
    });
    assert!(requires_source(&marked));
}
