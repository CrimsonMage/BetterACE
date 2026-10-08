use bace_inventory::{InventoryCylinder, inventory_pickup_motion, inventory_use_distance};
#[test]
fn original_ace_pickup_and_cylinder_methods_match_140_boundary_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/physical.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split_whitespace().collect();
        let f = |i: usize| p[i].parse::<f32>().unwrap();
        match p[0] {
            "motion" => assert_eq!(
                inventory_pickup_motion(f(1), f(2), f(3), f(4), p[5] == "1"),
                Some(p[6].parse().unwrap()),
                "{line}"
            ),
            "distance" => {
                let actual = inventory_use_distance(
                    InventoryCylinder {
                        position: [f(1), f(2), f(3)],
                        radius: f(4),
                        height: f(5),
                    },
                    InventoryCylinder {
                        position: [f(6), f(7), f(8)],
                        radius: f(9),
                        height: f(10),
                    },
                )
                .unwrap();
                let expected = p[11].parse::<f64>().unwrap();
                assert!((actual - expected).abs() < 1e-6, "{line}: {actual}");
            }
            _ => panic!("invalid independent fixture"),
        }
        count += 1;
    }
    assert_eq!(count, 140);
    assert_eq!(inventory_pickup_motion(f32::NAN, 2., 0., 1., false), None);
}
