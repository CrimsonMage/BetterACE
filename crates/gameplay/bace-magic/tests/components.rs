use bace_magic::foci_formula;
#[test]
fn original_ace_foci_formula_vectors() {
    for row in include_str!("fixtures/foci.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (input, expected) = row.split_once('=').unwrap();
        let parse = |s: &str| {
            s.split(';')
                .filter(|v| !v.is_empty())
                .map(|v| v.parse::<u32>().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            foci_formula(&parse(input)).unwrap(),
            parse(expected),
            "{row}"
        );
    }
    assert!(foci_formula(&[1; 9]).is_err());
}
