//! Expected WCIDs come from original C# field initializers executed by .NET,
//! independently of the declaration walker and Rust table-dispatch implementation.
mod table_support;
#[test]
fn cold_template_closure_covers_original_csharp_wcid_and_gem_leaves() {
    table_support::install_tables();
    let mut expected = std::collections::BTreeSet::new();
    for line in include_str!("fixtures/ace-tables.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('|').collect();
        if fields[0] == "gem"
            || (matches!(fields[0], "chance" | "sequence")
                && fields[1].split('.').next().unwrap().contains("Wcids")
                && fields[1] != "SpellComponentWcids.level8SpellComponentChance")
        {
            expected.insert(fields[2].parse::<u32>().unwrap());
        }
    }
    let actual = bace_loot::pinned_treasure_templates().unwrap();
    assert!(actual.len() > 1400 && actual.len() < 2000);
    assert_eq!(actual, expected.into_iter().collect::<Vec<_>>());
    assert!(
        !actual.contains(&1),
        "Boolean chance selectors are not template IDs"
    );
}
