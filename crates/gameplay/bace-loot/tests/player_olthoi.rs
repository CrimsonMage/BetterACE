use bace_loot::{TreasureError, TreasureRandom, roll_player_gland, roll_player_slag};
#[derive(Clone, Debug, PartialEq)]
struct Tape {
    case: u32,
    calls: u32,
    units: u32,
}
impl TreasureRandom for Tape {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.calls += 1;
        let result = if self.units < self.case % 3 {
            0.01
        } else {
            0.5
        };
        self.units += 1;
        Ok(result)
    }
    fn inclusive(&mut self, a: i32, b: i32) -> Result<i32, TreasureError> {
        self.calls += 1;
        Ok(if self.case.is_multiple_of(2) { a } else { b })
    }
}
#[test]
fn original_player_slag_and_gland_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/player_olthoi.trace")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let v: Vec<_> = line.split('|').collect();
        let mut tape = Tape {
            case: v[1].parse().unwrap(),
            calls: 0,
            units: 0,
        };
        if v[0] == "S" {
            let level = v[2].parse().unwrap();
            let elapsed: i32 = v[3].parse().unwrap();
            let vitae = v[4] == "True";
            let result = roll_player_slag(level, vitae, 10000 - elapsed, 10000, &mut tape).unwrap();
            assert_eq!(
                result.map_or(0, |r| r.stack),
                v[5].parse::<u32>().unwrap(),
                "{line}"
            );
            assert_eq!(
                result.map_or(10000 - elapsed, |r| r.timestamp),
                v[6].parse::<i32>().unwrap(),
                "{line}"
            );
            assert_eq!(tape.calls, v[7].parse::<u32>().unwrap(), "{line}");
        } else {
            assert_eq!(
                roll_player_gland(v[2] == "True", &mut tape).unwrap(),
                v[3] != "0"
            );
            assert_eq!(tape.calls, v[4].parse::<u32>().unwrap());
        }
        count += 1;
    }
    assert_eq!(count, 100);
}
#[derive(Clone, Debug, PartialEq)]
struct Repeat(u32);
impl TreasureRandom for Repeat {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.0 += 1;
        Ok(0.)
    }
    fn inclusive(&mut self, _: i32, b: i32) -> Result<i32, TreasureError> {
        self.0 += 1;
        Ok(b)
    }
}
#[test]
fn adversarial_repetition_preserves_random_cursor() {
    let mut tape = Repeat(0);
    assert_eq!(
        roll_player_slag(275, false, 0, 10000, &mut tape),
        Err(TreasureError::Capacity)
    );
    assert_eq!(tape, Repeat(0));
    let mut tape = Tape {
        case: 1,
        calls: 0,
        units: 0,
    };
    assert!(roll_player_slag(275, false, -1, 10, &mut tape).is_err());
    assert_eq!(tape.calls, 0);
}
