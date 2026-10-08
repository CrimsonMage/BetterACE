use bace_social::GagState;
#[test]
fn duration_and_notices_match_original_gags_tick() {
    let mut count = 0;
    for line in include_str!("fixtures/gag.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split('|').collect();
        let before = GagState {
            active: row[0] == "True",
            noticed: row[1] == "True",
            remaining: row[2].parse().unwrap(),
            timestamp: 123.,
        };
        let change = before.heartbeat(row[3].parse().unwrap()).unwrap();
        assert_eq!(change.after.active, row[4] == "True");
        assert_eq!(change.after.noticed, row[5] == "True");
        assert_eq!(change.after.remaining, row[6].parse::<f64>().unwrap());
        assert_eq!(change.after.timestamp, row[7].parse::<f64>().unwrap());
        assert_eq!(change.notices.suspended, row[8] == "1");
        assert_eq!(change.notices.restored, row[9] == "1");
        assert_eq!(change.notices.restored, row[10] == "1");
        count += 1;
    }
    assert_eq!(count, 80);
}
#[test]
fn source_manual_ungag_does_not_reset_transient_notice_and_reapplication_is_exact() {
    let original = GagState {
        active: true,
        timestamp: 1.,
        remaining: 10.,
        noticed: true,
    };
    let ungag = original.change(false, 2.).unwrap();
    assert!(ungag.after.noticed);
    let gag = ungag.after.change(true, 3.).unwrap();
    assert_eq!(gag.after.remaining, 300.);
    assert!(!gag.after.heartbeat(5.).unwrap().notices.suspended);
    let mut state = original;
    ungag.apply(&mut state).unwrap();
    assert!(ungag.apply(&mut state).is_err());
    assert!(state.change(true, f64::NAN).is_err());
    assert!(state.heartbeat(0.).is_err());
}
