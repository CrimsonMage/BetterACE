use bace_storage_codec::npc_values_v1::{NpcPropertyFamilyV1 as F, NpcValueV1 as V};
use bace_storage_codec::npc_workflow_v3::*;
#[test]
fn live_source_snapshot_rejects_duplicate_keys_wrong_family_and_nonfinite() {
    let row = NpcArchivedPropertyV3 {
        family: F::Float,
        stat: 54,
        value: V::Float(0.6),
    };
    let mut snapshot = NpcLivePropertiesV3 {
        revision: 19,
        properties: vec![row.clone()],
    };
    snapshot.validate().unwrap();
    snapshot.properties.push(row);
    assert!(snapshot.validate().is_err());
    snapshot.properties.pop();
    snapshot.properties[0].value = V::Int(1);
    assert!(snapshot.validate().is_err());
    snapshot.properties[0].value = V::Float(f64::INFINITY);
    assert!(snapshot.validate().is_err());
}
#[test]
fn source_quest_snapshot_rejects_aliases_and_retains_negative_progress() {
    let mut snapshot = NpcSourceQuestsV3 {
        revision: 41,
        entries: vec![NpcQuestEntryV3 {
            name: "FLAG".into(),
            last_completed_seconds: 23,
            completions: -2,
        }],
    };
    snapshot.validate().unwrap();
    snapshot.entries[0].name = "flag".into();
    assert!(snapshot.validate().is_err());
    snapshot.entries[0].name = "FLAG@comment".into();
    assert!(snapshot.validate().is_err());
}
