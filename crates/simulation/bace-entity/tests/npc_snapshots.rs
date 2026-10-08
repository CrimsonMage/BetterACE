use bace_entity::{EntityProperties, PropertyFamily as F, PropertyValue as V};
#[test]
fn explicit_property_snapshot_restores_revision_and_rejects_bad_schema() {
    let state = EntityProperties::restore_snapshot(
        91,
        vec![
            (F::Int, 25, V::Int(12)),
            (F::String, 1, V::String("Keeper".into())),
        ],
    )
    .unwrap();
    let (revision, values) = state.snapshot();
    assert_eq!(revision, 91);
    assert_eq!(values.len(), 2);
    let restored = EntityProperties::restore_snapshot(revision, values.clone()).unwrap();
    assert_eq!(restored, state);
    let mut duplicate = values.clone();
    duplicate.push(values[0].clone());
    assert!(EntityProperties::restore_snapshot(revision, duplicate).is_err());
    assert!(
        EntityProperties::restore_snapshot(0, vec![(F::Int, 25, V::String("invalid".into()))])
            .is_err()
    );
    assert!(
        EntityProperties::restore_snapshot(0, vec![(F::Float, 5, V::Float(f64::NAN))]).is_err()
    );
    assert!(EntityProperties::restore_snapshot(0, vec![(F::Int, 65536, V::Int(0))]).is_err());
    assert_eq!(state.revision(), 91);
}
