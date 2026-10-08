use bace_social::{ChatEligibility, ChatPolicy, PublicChatGate};
#[test]
fn source_public_gate_order_and_rejection_notices() {
    let mut count = 0;
    for line in include_str!("fixtures/public_chat.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split(',').collect();
        let v: Vec<u32> = fields[..5].iter().map(|v| v.parse().unwrap()).collect();
        let mut policy = ChatPolicy::default();
        match v[0] {
            1 => policy.echo_only = true,
            2 => policy.requires_account_15days = true,
            3 => policy.requires_account_time_seconds = 100,
            4 => policy.requires_player_age = 100,
            5 => policy.requires_player_level = 10,
            6 => policy.disable_general = true,
            7 => {
                policy.disable_trade = true;
                policy.echo_reject = true;
            }
            8 => {
                policy.requires_player_age = 100;
                policy.inform_reject = false;
            }
            _ => {}
        }
        let evidence = ChatEligibility {
            account_created_unix: Some(if v[1] >= 1 { 900 } else { 901 }),
            account_15_days: v[1] >= 1,
            player_age_seconds: if v[1] >= 2 { 100 } else { 99 },
        };
        let gate = policy.evaluate(v[2], v[2], evidence, if v[1] >= 3 { 10 } else { 9 }, 1000);
        let (echo, notice) = match gate {
            PublicChatGate::Deliver | PublicChatGate::Echo => (1, String::new()),
            PublicChatGate::Reject { reason, inform } => {
                let notice = if inform {
                    format!(
                        "{} is currently disabled for you {reason}.",
                        match v[2] {
                            2 => "General",
                            3 => "Trade",
                            4 => "LFG",
                            _ => "Roleplay",
                        }
                    )
                } else {
                    String::new()
                };
                (u32::from(policy.echo_reject), notice)
            }
        };
        assert_eq!(echo, v[3], "{line}");
        assert_eq!(v[4], 1, "{line}");
        assert_eq!(notice, fields[5], "{line}");
        count += 1;
    }
    assert_eq!(count, 144);
}
#[test]
fn unknown_historical_account_fails_closed_only_when_required_and_olthoi_skips_age() {
    let eligibility = ChatEligibility::default();
    let mut policy = ChatPolicy::default();
    assert_eq!(
        policy.evaluate(2, 2, eligibility, 1, 1000),
        PublicChatGate::Deliver
    );
    policy.requires_account_time_seconds = 100;
    assert!(matches!(
        policy.evaluate(2, 2, eligibility, 1, 1000),
        PublicChatGate::Reject { .. }
    ));
    assert_eq!(
        policy.evaluate(10, 10, eligibility, 1, 1000),
        PublicChatGate::Deliver
    );
    policy.echo_only = true;
    assert_eq!(
        policy.evaluate(2, 2, eligibility, 1, 1000),
        PublicChatGate::Echo
    );
    policy.disable_olthoi = true;
    assert_eq!(
        policy.evaluate(10, 10, eligibility, 1, 1000),
        PublicChatGate::Reject {
            reason: String::new(),
            inform: true
        }
    );
}
