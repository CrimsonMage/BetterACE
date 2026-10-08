use bace_inventory::{
    ActivationFailure as F, ActivationObject as O, ActivationRequirements as R, ActivationValues,
    WieldValues, check_item_activation,
};
struct Values {
    heritage: u32,
    training: u32,
    cooldown: bool,
}
impl WieldValues for Values {
    fn skill(&self, key: i32) -> Option<(u32, u32, u32)> {
        matches!(key, 14 | 54).then_some((50, 100, self.training))
    }
    fn attribute(&self, key: i32) -> Option<(u32, u32)> {
        (key == 1).then_some((50, 100))
    }
    fn vital(&self, key: i32) -> Option<(u32, u32)> {
        (key == 1).then_some((50, 100))
    }
    fn level(&self) -> i32 {
        50
    }
    fn int_property(&self, _: i32) -> i32 {
        0
    }
    fn bool_property(&self, _: i32) -> bool {
        false
    }
    fn creature_type(&self) -> i32 {
        31
    }
}
impl ActivationValues for Values {
    fn heritage(&self) -> u32 {
        self.heritage
    }
    fn mapped_skill(&self, key: i32) -> Option<u32> {
        u32::try_from(key).ok()
    }
    fn cooldown_ready(&self, group: Option<i32>) -> bool {
        group.is_none() || self.cooldown
    }
}
fn result(value: Result<(), F>) -> String {
    match value {
        Ok(()) => "ok".into(),
        Err(f) => match f {
            F::SkillLow(id) => format!("error:{}:{id}", 0x4c9),
            F::SkillUntrained(id) => {
                format!("transient:You must have {id} trained to use that item's magic")
            }
            F::SkillUnspecialized(id) => format!("error:{}:{id}", 0x4cb),
            F::AttributeLow(id) | F::VitalLow(id) => format!("error:{}:{id}", 0x4c9),
            F::Heritage(id) => format!("error:{}:{id}", 0x4c6),
            F::Level => "transient:You are not high enough level to use that!".into(),
            F::Cooldown => "transient:You have used this item too recently".into(),
            F::OlthoiInteract => format!("error:{}", 0x587),
            F::OlthoiLifestone => format!("error:{}", 0x588),
            F::OlthoiVendor => format!("error:{}", 0x589),
            F::OlthoiCreature => format!("error:{}:target", 0x58a),
            F::MissingValue => "missing".into(),
        },
    }
}
#[test]
fn compiled_original_ace_activation_order_limits_training_and_subtypes() {
    let mut count = 0;
    for row in include_str!("fixtures/activation.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let w: Vec<_> = row.split('|').collect();
        let n = |i: usize| w[i].parse::<i32>().unwrap();
        let mut r = R::default();
        let mut v = Values {
            heritage: 3,
            training: 2,
            cooldown: true,
        };
        let expected = match w[0] {
            "requirement" => {
                let d = Some(n(2));
                v.training = n(3) as u32;
                match n(1) {
                    0 => r.difficulty = d,
                    1 => {
                        r.item_skill = Some(54);
                        r.item_skill_level = d;
                    }
                    2 => {
                        r.skill = Some(54);
                        r.skill_level = d;
                    }
                    3 => {
                        r.specialized = Some(54);
                        r.skill_level = d;
                    }
                    4 => {
                        r.item_specialized = Some(54);
                        r.item_skill_level = d;
                    }
                    5 => r.level = d,
                    6 => {
                        r.attribute = Some(1);
                        r.attribute_level = d;
                    }
                    7 => {
                        r.vital = Some(1);
                        r.vital_level = d;
                    }
                    8 => r.skill = Some(54),
                    9 => r.item_skill = Some(54),
                    10 => r.attribute = Some(1),
                    11 => r.vital = Some(1),
                    12 => {
                        r.difficulty = Some(101);
                        r.level = Some(101);
                        r.cooldown = Some(1);
                        v.cooldown = false;
                    }
                    _ => panic!("mode"),
                };
                w[4]
            }
            "object" => {
                v.heritage = n(1) as u32;
                r.heritage = n(3) as u32;
                r.cooldown = Some(1);
                v.cooldown = n(4) != 0;
                r.object = match n(2) {
                    0 | 7 => O::Ordinary,
                    1 => O::Creature {
                        olthoi: true,
                        vendor: false,
                        looks_like_object: false,
                    },
                    2 => O::Creature {
                        olthoi: false,
                        vendor: true,
                        looks_like_object: false,
                    },
                    3 => O::Creature {
                        olthoi: false,
                        vendor: false,
                        looks_like_object: true,
                    },
                    4 => O::Creature {
                        olthoi: false,
                        vendor: false,
                        looks_like_object: false,
                    },
                    5 => O::Lifestone,
                    6 => O::RestrictedOlthoi,
                    _ => panic!("object"),
                };
                w[5]
            }
            _ => panic!("row"),
        };
        assert_eq!(result(check_item_activation(&r, &v)), expected, "{row}");
        count += 1;
    }
    assert_eq!(count, 404);
}
#[test]
fn malformed_missing_stats_fail_closed_and_messages_remain_typed() {
    let r = R {
        attribute: Some(99),
        ..Default::default()
    };
    assert_eq!(
        check_item_activation(
            &r,
            &Values {
                heritage: 3,
                training: 2,
                cooldown: true
            }
        ),
        Err(F::MissingValue)
    );
    assert_eq!(
        F::SkillLow(14).message("x"),
        Some(bace_inventory::ActivationMessage::ErrorWithString {
            error: 0x4c9,
            text: "Arcane Lore".into()
        })
    );
    assert_eq!(F::OlthoiCreature.message(&"x".repeat(513)), None);
}
