//! Golden excerpts from pinned official ACE `Source/ACE.Server/Entity/Strings.cs`.
//! The test checks selection and formatting against source text, not a round trip.
use bace_combat::death_messages::{
    death_message_templates, format_death_message, pk_critical_killer_template,
    unattended_death_template,
};

#[test]
fn pinned_death_text_selection_and_substitution() {
    let cases = [
        (0x1, false, 4, "You split {0} apart!"),
        (0x2, false, 4, "You run {0} through!"),
        (0x4, false, 4, "You beat {0} to a lifeless pulp!"),
        (0x8, false, 3, "Your attack stops {0} cold!"),
        (0x10, false, 4, "You bring {0} to a fiery end!"),
        (0x20, false, 3, "{0} is liquified by your attack!"),
        (0x40, false, 3, "Blistered by lightning, {0} falls!"),
        (0x400, false, 3, "{0} is dessicated by your attack!"),
        (0x4, true, 7, "You obliterate {0}!"),
    ];
    for (kind, critical, count, first) in cases {
        let templates = death_message_templates(kind, critical);
        assert_eq!(templates.len(), count);
        assert_eq!(templates[0].killer, first);
    }
    assert_eq!(
        format_death_message(
            death_message_templates(0x1, false)[0].broadcast,
            "Aster",
            "Birch"
        ),
        "Birch splits Aster apart!"
    );
    assert_eq!(unattended_death_template().victim, "You died!");
    assert_eq!(
        pk_critical_killer_template().killer,
        "You send {0} to death so violently that even the lifestone flinches!"
    );
    assert_eq!(
        death_message_templates(0x3, false)[0],
        unattended_death_template()
    );
}
