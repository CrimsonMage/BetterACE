use bace_loot::{CreateEntry, LootError, select_create_list};
fn entries() -> Vec<CreateEntry> {
    [
        (1, 0.7),
        (8, 0.25),
        (8, 0.75),
        (8, 0.0),
        (8, 0.5),
        (8, 0.5),
        (2, 0.4),
    ]
    .iter()
    .enumerate()
    .map(|(i, (destination, shade))| CreateEntry {
        template: i as u32,
        destination: *destination,
        shade: *shade,
    })
    .collect()
}
#[test]
fn official_create_list_vectors() {
    for line in include_str!("fixtures/create-list.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let mut output = Vec::with_capacity(8);
        let consumed = select_create_list(
            &entries(),
            &[p[0].parse().unwrap(), p[1].parse().unwrap()],
            &mut output,
        )
        .unwrap();
        let expected: Vec<usize> = p[2].split(';').map(|n| n.parse().unwrap()).collect();
        assert_eq!(output, expected, "{line}");
        assert_eq!(consumed, 2);
    }
}
#[test]
fn malformed_or_short_random_stream_retains_output() {
    let mut output = Vec::with_capacity(8);
    output.push(99);
    assert_eq!(
        select_create_list(&entries(), &[0.1], &mut output),
        Err(LootError::MissingRandom)
    );
    assert_eq!(
        select_create_list(&entries(), &[0.1, f32::NAN], &mut output),
        Err(LootError::InvalidRandom)
    );
    assert_eq!(output, [99]);
    let mut none = Vec::new();
    assert_eq!(
        select_create_list(&entries(), &[0.1, 0.1], &mut none),
        Err(LootError::OutputCapacity)
    );
}
