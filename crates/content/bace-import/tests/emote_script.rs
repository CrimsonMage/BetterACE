use bace_import::{export_emote_script, import_emote_script};
#[test]
fn scripts_preserve_exact_fields_and_explicit_branch_links() {
    let source = "Use:\n    - Tell: Hello, traveler!\n    - AwardXP: 9,007,199,254,740,993\n    - InqQuest: TestQuest\n        QuestSuccess:\n            - Tell: Already complete.\n        QuestFailure:\n            - SetQuestCompletions: TestQuest, 0\n";
    let emotes = import_emote_script(source).unwrap();
    assert_eq!(emotes.len(), 3);
    assert_eq!(emotes[0].actions[1].amount64, Some(9_007_199_254_740_993));
    assert!(
        emotes[1]
            .quest
            .as_deref()
            .unwrap()
            .starts_with("TestQuest@studio_")
    );
    let script = export_emote_script(&emotes).unwrap();
    assert_eq!(import_emote_script(&script).unwrap(), emotes);
}
#[test]
fn syntax_errors_do_not_yield_partial_content() {
    for source in [
        "Oops:",
        "    - Tell: no event",
        "Use:\n    - FakeAction: 1",
        "Use:\n    - Tell: \"unterminated",
        "Use:\n    - AwardXP: Amount64: 9223372036854775808",
        "Use:\n    - Tell: hi\n        QuestSuccess:\n            - Tell: no branch",
    ] {
        assert!(import_emote_script(source).is_err(), "{source}");
    }
}

#[test]
fn repeated_quest_queries_have_distinct_nested_branches() {
    let source = "Use:\n    - InqQuest: counter\n        QuestSuccess:\n            - Tell: first\n    - InqQuestSolves: counter, 300\n        QuestSuccess:\n            - Tell: second\n";
    let sets = import_emote_script(source).unwrap();
    assert_ne!(sets[1].quest, sets[2].quest);
    assert_eq!(sets[0].actions[0].message, sets[1].quest);
    assert_eq!(sets[0].actions[1].message, sets[2].quest);
    assert_eq!(sets[0].actions[1].min, Some(300));
    assert_eq!(
        import_emote_script(&export_emote_script(&sets).unwrap()).unwrap(),
        sets
    );
}

#[test]
fn movement_shorthand_keeps_cell_origin_and_wxyz_order() {
    let sets = import_emote_script("Use:\n    - TeleportTarget: 0xA9B40019 [10.25 -2 40] 1 0 0 0\n    - Move: [1 2 3]\n    - Turn: West\n").unwrap();
    let p = &sets[0].actions[0];
    assert_eq!(p.obj_cell_id, Some(0xa9b40019));
    assert_eq!(
        (p.origin_x, p.origin_y, p.origin_z),
        (Some(10.25), Some(-2.0), Some(40.0))
    );
    assert_eq!(p.angles_w, Some(1.0));
    assert_eq!(sets[0].actions[1].angles_w, None);
    assert!(
        (sets[0].actions[2].angles_z.unwrap() - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.00001
    );
    assert!(import_emote_script("Use:\n    - TeleportTarget: 0xA9B40019 [1 2] 1 0 0 0").is_err());
}

#[test]
fn upstream_cow_example_matches_checked_in_expected_values() {
    // ACEmulator/EmoteScript aa22635cecc32f534d3630882e96dada38dd6f6d,
    // EmoteScript.Tests/Scripts/cow.{es,json}, LGPL-3.0 (docs/licenses).
    let actual = import_emote_script(include_str!(
        "../../../../tests/fixtures/content/emotescript-cow.es"
    ))
    .unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/content/emotescript-cow.json"
    ))
    .unwrap();
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            i64::from(actual.category),
            expected["category"].as_i64().unwrap()
        );
        if let Some(quest) = expected["quest"].as_str() {
            // Branch keys are intentionally generated differently; the quest lookup
            // prefix and links must survive (links have their own regression above).
            assert_eq!(
                actual.quest.as_ref().unwrap().split('@').next(),
                quest.split('@').next()
            );
        }
        let actions = expected["emotes"].as_array().unwrap();
        assert_eq!(actual.actions.len(), actions.len());
        for (actual, expected) in actual.actions.iter().zip(actions) {
            let value = serde_json::to_value(actual).unwrap();
            for (key, expected) in expected.as_object().unwrap() {
                let key = match key.as_str() {
                    "msg" => "message",
                    "wcid" | "classID" => "weenie_class_id",
                    other => other,
                };
                if key == "message"
                    && actual
                        .message
                        .as_ref()
                        .is_some_and(|m| m.contains("@studio_"))
                {
                    assert_eq!(
                        actual.message.as_ref().unwrap().split('@').next(),
                        expected.as_str().unwrap().split('@').next()
                    );
                } else {
                    assert_eq!(&value[key], expected, "{key}");
                }
            }
        }
    }
}
