//! Original ActOnUse fixture qualifies the existing domain policy/effect subset.
//! It does not qualify live Use routing, authored motion, or output serialization.
use bace_interactions::{
    BindingKind, PortalPosition, RecallError, RecallPolicy, check_binding, complete_binding,
};

#[test]
fn binding_permissions_final_radius_position_copy_and_stamina_match_original_ace() {
    let mut count = 0;
    for line in include_str!("fixtures/binding_output.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let inputs: Vec<u32> = fields[0].split(',').map(|v| v.parse().unwrap()).collect();
        let kind = if inputs[0] == 0 {
            BindingKind::Lifestone
        } else {
            BindingKind::Allegiance
        };
        let permission = inputs[3];
        let admitted = check_binding(
            &RecallPolicy::default(),
            kind,
            permission != 0,
            permission.saturating_sub(1),
        );
        // The original source returns before scheduling anything on permission
        // failure; this is observed in its LastUseTime and complete call trace.
        if fields[1] == "0" {
            assert!(admitted.is_err(), "{line}");
            assert!(fields[4].starts_with("0:error:"), "{line}");
        } else {
            admitted.unwrap();
            let mut accepted = PortalPosition {
                cell: 0x0101_0001,
                origin: [5.5, 6., 7.],
                rotation: [1., 0., 0., 0.],
            };
            let effect = complete_binding(kind, accepted, inputs[2] != 0, inputs[4]);
            if inputs[2] == 0 {
                assert_eq!(effect, Err(RecallError::MovedTooFar), "{line}");
                assert!(fields[4].ends_with(":error:1176"), "{line}");
            } else {
                let effect = effect.unwrap();
                accepted.origin[0] = 99.;
                let source_copy: f32 = fields[if kind == BindingKind::Lifestone { 2 } else { 3 }]
                    .parse()
                    .unwrap();
                assert_eq!(effect.sanctuary.origin[0], source_copy, "{line}");
                assert_ne!(effect.sanctuary.origin[0], accepted.origin[0]);
                let source_stamina = fields[4].split('|').find_map(|call| {
                    call.split_once(":stamina:")
                        .map(|(_, value)| value.parse::<u32>().unwrap())
                });
                assert_eq!(effect.stamina_after, source_stamina, "{line}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 480);
}
