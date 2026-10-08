use bace_magic::{VitalState, gdle_heal, gdle_natural_resistance, gdle_transfer};
#[test]
fn original_gdle_transfer_heal_and_natural_resistance_vectors() {
    for row in include_str!("fixtures/vitals.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut parts = row.split(',');
        let kind = parts.next().unwrap();
        let p: Vec<f64> = parts.map(|v| v.parse().unwrap()).collect();
        match kind {
            "transfer" => {
                let result = gdle_transfer(
                    VitalState {
                        current: p[0] as u32,
                        maximum: 100,
                    },
                    VitalState {
                        current: p[1] as u32,
                        maximum: 100,
                    },
                    p[2] as f32,
                    p[3] as f32,
                    p[4] as u32,
                    p[5],
                    p[6],
                );
                if p[7] > p[0] {
                    assert!(result.is_err(), "untrusted overdraw {row}");
                } else {
                    let result = result.unwrap();
                    assert_eq!(
                        (
                            result.source.before - result.source.after,
                            result.destination.after - result.destination.before
                        ),
                        (p[7] as u32, p[8] as u32),
                        "{row}"
                    );
                }
            }
            "heal" => assert_eq!(
                gdle_heal(
                    VitalState {
                        current: p[0] as u32,
                        maximum: 100
                    },
                    p[1],
                    p[2] as f32,
                    true
                )
                .unwrap()
                .after,
                p[3] as u32,
                "{row}"
            ),
            "natural" => assert_eq!(
                gdle_natural_resistance(p[0] as u32, p[1] as u32)
                    .unwrap()
                    .to_bits(),
                (p[2] as f32).to_bits(),
                "{row}"
            ),
            _ => panic!("unknown vector {row}"),
        }
    }
}
