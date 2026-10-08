use bace_content::ClothingPatchV1;
use bace_content_tools::{compile_clothing_patch, decode_clothing_patch, parse_clothing_patch};

#[test]
fn native_clothing_record_keeps_identity_and_rejects_wrong_envelope() {
    let source = "schema_version = 1\nid = 268435457\n";
    let patch = parse_clothing_patch(source).unwrap();
    assert_eq!(patch.id, 0x10000001);
    let bytes = compile_clothing_patch(&patch).unwrap();
    assert_eq!(decode_clothing_patch(&bytes).unwrap(), patch);
    let wrong = bace_storage_codec::encode(22, 1, &patch, Default::default()).unwrap();
    assert!(decode_clothing_patch(&wrong).is_err());
    let mut corrupt = bytes;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(decode_clothing_patch(&corrupt).is_err());
    assert!(compile_clothing_patch(&ClothingPatchV1 { id: 1, ..patch }).is_err());
}
