use bace_content::Emote;
use bace_emotes::{NativeLimits, NativeProgram, NativeTrigger};
fn program() -> NativeProgram {
    let mut sets = Vec::new();
    let mut add = |id: u32,
                   cat: i32,
                   probability: f32,
                   quest: Option<&str>,
                   style: Option<u32>,
                   motion: Option<u32>,
                   min: Option<f32>,
                   max: Option<f32>,
                   vendor: Option<i32>| {
        sets.push(Emote {
            database_record_id: id,
            category: cat,
            probability,
            quest: quest.map(str::to_owned),
            style,
            substyle: motion,
            min_health: min,
            max_health: max,
            vendor_type: vendor,
            weenie_class_id: Some(if id.is_multiple_of(2) { 100 } else { 200 }),
            ..Default::default()
        });
    };
    add(1, 7, 0.25, Some("foo"), None, None, None, None, None);
    add(2, 7, 0.75, Some("foo"), None, None, None, None, None);
    add(3, 7, 0.75, Some("FOO"), None, None, None, None, None);
    add(4, 7, 1., None, None, None, None, None, None);
    add(10, 24, 0.5, None, None, None, None, None, None);
    add(11, 24, 0.8, Some("foo"), None, None, None, None, None);
    add(12, 24, 0.2, Some("other"), None, None, None, None, None);
    add(20, 38, 0.4, Some("ß"), None, None, None, None, None);
    add(21, 38, 0.6, Some("ss"), None, None, None, None, None);
    add(22, 38, 0.8, Some("ẞ"), None, None, None, None, None);
    add(23, 38, 0.3, Some("ı"), None, None, None, None, None);
    add(24, 38, 0.7, Some("I"), None, None, None, None, None);
    add(25, 38, 0.6, Some("σ"), None, None, None, None, None);
    add(30, 5, 0.9, None, None, None, None, None, None);
    add(
        31,
        5,
        0.4,
        None,
        Some(0x8000003d),
        Some(0x40000003),
        None,
        None,
        None,
    );
    add(32, 5, 0.2, None, Some(0x80000040), None, None, None, None);
    add(40, 15, 0.1, None, None, None, None, None, None);
    add(41, 15, 0.5, None, None, None, Some(0.), Some(0.5), None);
    add(42, 15, 0.8, None, None, None, Some(0.5), Some(1.), None);
    add(50, 2, 0.5, None, None, None, None, None, Some(1));
    add(51, 2, 0.7, None, None, None, None, None, Some(2));
    NativeProgram::prepare(sets, NativeLimits::default()).unwrap()
}
#[test]
fn pinned_category_selection_order_filters_ties_and_unicode_match() {
    let program = program();
    let ids = [
        1, 2, 3, 4, 10, 11, 12, 20, 21, 22, 23, 24, 25, 30, 31, 32, 40, 41, 42, 50, 51,
    ];
    let quests = [
        None,
        Some("foo"),
        Some("FOO"),
        Some("other"),
        Some("ß"),
        Some("ẞ"),
        Some("ss"),
        Some("ı"),
        Some("I"),
        Some("i"),
        Some("İ"),
        Some("Σ"),
        Some("ς"),
    ];
    for line in include_str!("fixtures/native_selection.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<usize> = line.split(',').map(|n| n.parse().unwrap()).collect();
        let trigger = NativeTrigger {
            category: [7, 24, 38, 5, 15, 2][p[0]],
            quest: quests[p[1]].map(str::to_owned),
            vendor: if p[3] == 0 { None } else { Some(p[3] as i32) },
            template: match p[4] {
                0 => None,
                1 => Some(100),
                _ => Some(777),
            },
            style: Some(0x8000003d),
            motion: Some(0x40000003),
            health_fraction: Some(if p[5] == 0 { 0.5 } else { 0.8 }),
            random: true,
        };
        let result = program
            .select(&trigger, Some([0., 0.25, 0.5, 0.9999999999999999][p[2]]))
            .unwrap();
        assert_eq!(result.map_or(0, |i| ids[i]), p[6], "source vector {line}");
    }
}
