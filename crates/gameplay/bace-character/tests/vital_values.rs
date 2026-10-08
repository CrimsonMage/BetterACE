use bace_character::{
    VitalFormula, VitalValueInputs, project_attribute_value, project_vital_values,
};
#[test]
fn original_ace_vital_and_attribute_methods_match_all_golden_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/vitals.csv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let fields: Vec<_> = line.split(',').collect();
        if fields[0] == "a" {
            assert_eq!(
                project_attribute_value(
                    fields[1].parse().unwrap(),
                    fields[2].parse().unwrap(),
                    fields[3].parse().unwrap()
                )
                .unwrap(),
                fields[4].parse::<u32>().unwrap(),
                "{line}"
            );
        } else {
            let mut attributes = [0; 6];
            for (index, attribute) in attributes.iter_mut().enumerate() {
                *attribute = project_attribute_value(24 + index as u32, 1.1, -7).unwrap();
            }
            let value = project_vital_values(VitalValueInputs {
                formula: VitalFormula {
                    enabled: true,
                    attribute1: 2,
                    attribute2: 0,
                    divisor: 2,
                },
                starting_value: fields[1].parse().unwrap(),
                ranks: fields[2].parse().unwrap(),
                base_bonus: fields[3].parse().unwrap(),
                current_attributes: attributes,
                multiplier: fields[4].parse().unwrap(),
                vitae: fields[5].parse().unwrap(),
                additive: fields[6].parse().unwrap(),
            })
            .unwrap();
            assert_eq!(value.maximum, fields[7].parse::<u32>().unwrap(), "{line}");
        }
        count += 1;
    }
    assert_eq!(count, 366);
}
#[test]
fn invalid_vital_math_does_not_wrap_or_accept_nonfinite_modifiers() {
    let mut input = VitalValueInputs {
        formula: VitalFormula {
            enabled: false,
            attribute1: 0,
            attribute2: 0,
            divisor: 0,
        },
        starting_value: 5,
        ranks: 0,
        current_attributes: [0; 6],
        base_bonus: 0,
        multiplier: 1.,
        vitae: 1.,
        additive: 0.,
    };
    input.vitae = f32::NAN;
    assert!(project_vital_values(input).is_err());
    input.vitae = 1.;
    input.starting_value = u32::MAX;
    input.ranks = 1;
    assert!(project_vital_values(input).is_err());
    assert!(project_attribute_value(u32::MAX, 1., 0).is_err());
    assert_eq!(project_attribute_value(9, 0., -100).unwrap(), 1);
    assert_eq!(project_attribute_value(10, 0., -100).unwrap(), 10);
}
