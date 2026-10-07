use super::*;
#[test]
fn claimed_coverage_needs_real_evidence_and_known_owner() {
    let temp = tempfile::tempdir().unwrap();
    let mut inventory = Inventory {
        version: 1,
        repository: "https://github.com/ACEmulator/ACE".into(),
        commit: "pin".into(),
        source_count: 1,
        sources: vec![Source {
            source: "Source/Network.cs".into(),
            sha256: "a".repeat(64),
            git_blob: "b".repeat(40),
            owner: "bace-wire".into(),
            status: "unsupported".into(),
            evidence: vec![],
        }],
    };
    let owners = BTreeSet::from(["bace-wire"]);
    assert!(validate(temp.path(), &inventory, "pin", &owners).is_empty());
    inventory.sources[0].status = "implemented".into();
    assert!(!validate(temp.path(), &inventory, "pin", &owners).is_empty());
    inventory.sources[0].evidence = vec!["fixture.json".into()];
    assert!(!validate(temp.path(), &inventory, "pin", &owners).is_empty());
    fs::write(temp.path().join("fixture.json"), "{}").unwrap();
    assert!(validate(temp.path(), &inventory, "pin", &owners).is_empty());
    inventory.sources[0].owner = "unassigned".into();
    assert!(!validate(temp.path(), &inventory, "pin", &owners).is_empty());
}
