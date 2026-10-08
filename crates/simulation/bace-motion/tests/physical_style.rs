use bace_motion::{ActionChainRequest, MotionTableSource, resolve_sequence};
use std::collections::BTreeMap;
struct Data {
    count: usize,
    id: u32,
}
struct Table {
    defaults: BTreeMap<u32, u32>,
    links: BTreeMap<(u32, u32), Data>,
    cycles: BTreeMap<u32, Data>,
}
impl MotionTableSource for Table {
    type Data = Data;
    fn default_motion(&self, style: u32) -> Option<u32> {
        self.defaults.get(&style).copied()
    }
    fn link(&self, key: u32, motion: u32) -> Option<&Data> {
        self.links.get(&(key, motion))
    }
    fn cycle(&self, key: u32) -> Option<&Data> {
        self.cycles.get(&key)
    }
    fn animation_count(&self, data: &Data) -> usize {
        data.count
    }
    fn default_style(&self) -> u32 {
        0x8000003d
    }
    fn bitfield(&self, _: &Data) -> u8 {
        0
    }
}
#[test]
fn unchanged_gdle_style_branch_vectors() {
    let mut count = 0;
    for row in include_str!("fixtures/physical_style.csv")
        .lines()
        .filter(|r| !r.starts_with('#'))
    {
        let v: Vec<_> = row.split(',').collect();
        let scenario: u32 = v[0].parse().unwrap();
        let speed: f32 = v[1].parse().unwrap();
        let from = 0x80000049u32;
        let to = 0x80000040u32;
        let ready = 0x41000003u32;
        let other = 0x40000014u32;
        let mut t = Table {
            defaults: BTreeMap::from([
                (from, ready),
                (to, if scenario == 4 { other } else { ready }),
                (0x8000003d, ready),
            ]),
            links: BTreeMap::new(),
            cycles: BTreeMap::new(),
        };
        let mut link = |style: u32, current: u32, dest: u32, id, count| {
            t.links.insert(
                (style.wrapping_shl(16) | (current & 0xffffff), dest),
                Data { id, count },
            );
        };
        if scenario != 2 && scenario != 3 {
            link(from, ready, to, 11, 2);
        }
        if scenario == 1 || scenario == 6 {
            link(from, other, ready, 12, 1);
            link(from, ready, other, 12, 1);
        }
        if scenario == 2 {
            link(from, ready, 0x8000003d, 13, 3);
            link(0x8000003d, ready, to, 15, 2);
        }
        if scenario != 8 {
            t.cycles.insert(
                to.wrapping_shl(16) | (ready & 0xffffff),
                Data {
                    id: 14,
                    count: if scenario == 7 { 3 } else { 1 },
                },
            );
        }
        t.cycles.insert(
            from.wrapping_shl(16) | (ready & 0xffffff),
            Data { id: 14, count: 1 },
        );
        let result = resolve_sequence(
            &t,
            ActionChainRequest {
                style: from,
                current: if scenario == 1 || scenario == 6 {
                    other
                } else {
                    ready
                },
                current_speed: if scenario == 6 { -1.0 } else { 1.0 },
                action: if scenario == 5 { from } else { to },
                action_speed: speed,
            },
        );
        if v[2] == "0" {
            assert!(result.is_err(), "{row}");
        } else {
            let result = result.unwrap();
            assert_eq!(result.final_style, v[3].parse().unwrap(), "{row}");
            assert_eq!(result.final_substate, v[4].parse().unwrap());
            assert_eq!(result.final_speed, v[5].parse().unwrap());
            assert_eq!(result.completion_clips, v[6].parse().unwrap());
            if scenario == 5 {
                assert!(result.continues_cycle);
            } else {
                let actual: Vec<_> = result
                    .parts
                    .iter()
                    .map(|p| format!("{}:{}", p.data.id, p.speed))
                    .collect();
                assert_eq!(actual, v[7..], "{row}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 36);
}
