use bace_character::{NameError, NamePolicy};
fn policy(patterns: &[&str], creatures: &[&str]) -> NamePolicy {
    NamePolicy::prepare(
        &patterns.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        &creatures.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
#[test]
fn approved_names_have_exact_binding_and_selected_case_policy() {
    let p = policy(&[], &[]);
    assert_eq!(p.approve("ALICE").unwrap().normalized_name(), "Alice");
    assert_eq!(
        p.approve("Alice smith").unwrap().normalized_name(),
        "Alice smith"
    );
    assert_eq!(p.approve("alice"), Err(NameError::InvalidSyntax));
    for bad in [
        " Alice",
        "Alice ",
        "Alice\n",
        "Alice\tBob",
        "Alice  Bob",
        "Alice9",
    ] {
        assert!(p.approve(bad).is_err(), "{bad:?}");
    }
    assert_eq!(p.approve("Alice😀"), Err(NameError::NotRepresentable));
    assert!(p.approve("Élodie").is_ok());
    assert!(p.approve("O'Brian-Smith").is_ok());
    assert!(p.approve(&format!("A{}", "a".repeat(99))).is_ok());
    assert_eq!(
        p.approve(&format!("A{}", "a".repeat(100))),
        Err(NameError::TooLong)
    );
}
#[test]
fn taboo_patterns_are_word_local_case_insensitive_and_checked_after_normalization() {
    for (pattern, banned, allowed) in [
        ("foo", "Foo", "Foobar"),
        ("foo*", "Foobar", "Barfoo"),
        ("*foo", "Barfoo", "Foobar"),
        ("*foo*", "Barfoobar", "Fof"),
    ] {
        let p = policy(&[pattern], &[]);
        assert_eq!(p.approve(banned), Err(NameError::Banned));
        assert!(p.approve(allowed).is_ok());
        assert_eq!(
            p.approve(&format!("Alice {}", banned.to_uppercase())),
            Err(NameError::Banned)
        );
    }
    assert_eq!(
        policy(&[], &["Alice"]).approve("ALICE"),
        Err(NameError::CreatureName)
    );
    assert!(policy(&["a*b*c"], &[]).approve("Azzbzzc").is_err());
    assert_eq!(
        NamePolicy::prepare(&["a.b".into()], &[]).unwrap_err(),
        NameError::UnsupportedPattern
    );
}
#[test]
fn prepared_matching_agrees_with_verbatim_official_taboo_oracle() {
    let p = policy(&["foo", "*bar*"], &[]);
    let mut count = 0;
    for line in include_str!("fixtures/taboo.csv")
        .lines()
        .filter(|l| l.starts_with("match,"))
    {
        let fields: Vec<_> = line.split(',').collect();
        assert_eq!(
            p.approve(fields[1]) == Err(NameError::Banned),
            fields[2] == "1",
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 6);
}
