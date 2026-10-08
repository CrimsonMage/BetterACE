//! Death text from pinned official ACE `Source/ACE.Server/Entity/Strings.cs`.
//! Copyright ACE contributors. AGPL-3.0-only.
//! The caller supplies the accepted damage type and an explicit bounded draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathMessageTemplate {
    pub killer: &'static str,
    pub victim: &'static str,
    pub broadcast: &'static str,
}

const SLASHING: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "You split {0} apart!",
        victim: "{1} splits you apart!",
        broadcast: "{1} splits {0} apart!",
    },
    DeathMessageTemplate {
        killer: "You cleave {0} in twain!",
        victim: "{1} cleaves you in twain!",
        broadcast: "{1} cleaves {0} in twain!",
    },
    DeathMessageTemplate {
        killer: "{0} is torn to ribbons by your assault!",
        victim: "You are torn to ribbons by {1}'s assault!",
        broadcast: "{0} is torn to ribbons by {1}'s assault!",
    },
    DeathMessageTemplate {
        killer: "Your killing blow nearly turns {0} inside-out!",
        victim: "{1}'s killing blow nearly turns you inside-out!",
        broadcast: "{1}'s killing blow nearly turns {0} inside-out!",
    },
];

const PIERCING: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "You run {0} through!",
        victim: "{1} runs you through!",
        broadcast: "{1} runs {0} through!",
    },
    DeathMessageTemplate {
        killer: "{0} is fatally punctured!",
        victim: "You are fatally punctured by {1}!",
        broadcast: "{0} is fatally punctured by {1}!",
    },
    DeathMessageTemplate {
        killer: "{0}'s perforated corpse falls before you!",
        victim: "Your perforated corpse falls before {1}!",
        broadcast: "{0}'s perforated corpse falls before {1}!",
    },
    DeathMessageTemplate {
        killer: "{0}'s death is preceded by a sharp, stabbing pain!",
        victim: "Your death is preceded by a sharp, stabbing pain, courtesy of {1}!",
        broadcast: "{0}'s death is preceded by a sharp, stabbing pain, courtesy of {1}!",
    },
];

const BLUDGEONING: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "You beat {0} to a lifeless pulp!",
        victim: "{1} beats you to a lifeless pulp!",
        broadcast: "{1} beats {0} to a lifeless pulp!",
    },
    DeathMessageTemplate {
        killer: "{0} is shattered by your assault!",
        victim: "Your body is shattered by {1}'s attack!",
        broadcast: "{0}'s body is shattered by {1}'s attack!",
    },
    DeathMessageTemplate {
        killer: "You flatten {0}'s body with the force of your assault!",
        victim: "The force of {1}'s assault flattens you!",
        broadcast: "The force of {1}'s assault flattens {0}!",
    },
    DeathMessageTemplate {
        killer: "The thunder of crushing {0} is followed by the deafening silence of death!",
        victim: "The thunder of {1} crushing {0} is followed by the deafening silence of your death!",
        broadcast: "The thunder of {1} crushing {0} is followed by the deafening silence of death!",
    },
];

const FIRE: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "You bring {0} to a fiery end!",
        victim: "{1} brings you to a fiery end!",
        broadcast: "{1} brings {0} to a fiery end!",
    },
    DeathMessageTemplate {
        killer: "{0} is reduced to cinders!",
        victim: "You are reduced to cinders by {1}!",
        broadcast: "{1} reduced {0} to cinders!",
    },
    DeathMessageTemplate {
        killer: "{0} is incinerated by your assault!",
        victim: "You are incinerated by {1}'s assault!",
        broadcast: "{0} is incinerated by {1}'s assault!",
    },
    DeathMessageTemplate {
        killer: "{0}'s seared corpse smolders before you!",
        victim: "Your seared corpse smolders before {1}!",
        broadcast: "{0}'s seared corpse smolders before {1}!",
    },
];

const ICE: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "Your attack stops {0} cold!",
        victim: "{1}'s attack stops you cold!",
        broadcast: "{1}'s attack stops {0} cold!",
    },
    DeathMessageTemplate {
        killer: "Your assault sends {0} to an icy death!",
        victim: "{1}'s assault sends you to an icy death!",
        broadcast: "{1}'s assault sends {0} to an icy death!",
    },
    DeathMessageTemplate {
        killer: "{0} suffers a frozen fate!",
        victim: "You suffer a frozen fate at the hands of {1}!",
        broadcast: "{0} suffers a frozen fate at the hands of {1}!",
    },
];

const ACID: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "{0} is liquified by your attack!",
        victim: "You are liquified by {1}'s attack!",
        broadcast: "{0} is liquified by {1}'s attack!",
    },
    DeathMessageTemplate {
        killer: "{0}'s last strength dissolves before you!",
        victim: "Your last strength dissolves before {1}!",
        broadcast: "{0}'s last strength dissolves before {1}!",
    },
    DeathMessageTemplate {
        killer: "You reduce {0} to a sizzling, oozing mass!",
        victim: "{1} reduces you to a sizzling, oozing mass!",
        broadcast: "{1} reduces {0} to a sizzling, oozing mass!",
    },
];

const LIGHTNING: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "Blistered by lightning, {0} falls!",
        victim: "Blistered by {1}'s lightning, you die!",
        broadcast: "Blistered by {1}'s lightning, {0} dies!",
    },
    DeathMessageTemplate {
        killer: "Electricity tears {0} apart!",
        victim: "Electricity from {1}'s attack tears you apart!",
        broadcast: "Electricity from {1}'s attack tears {0} apart!",
    },
    DeathMessageTemplate {
        killer: "Your lightning coruscates over {0}'s mortal remains!",
        victim: "{1}'s lightning coruscates over your mortal remains!",
        broadcast: "{1}'s lightning coruscates over {0}'s mortal remains!",
    },
];

const VOID: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "{0} is dessicated by your attack!",
        victim: "You are dessicated by {1}'s attack!",
        broadcast: "{0} is dessicated by {1}'s attack!",
    },
    DeathMessageTemplate {
        killer: "{0}'s last strength withers before you!",
        victim: "Your last strength withers before {1}!",
        broadcast: "{0}'s last strength withers before {1}!",
    },
    DeathMessageTemplate {
        killer: "You reduce {0} to a drained, twisted corpse!",
        victim: "{1} reduces you to a drained, twisted corpse!",
        broadcast: "{1} reduces {0} to a drained, twisted corpse!",
    },
];

const CRITICAL: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "You obliterate {0}!",
        victim: "{1} obliterates you!",
        broadcast: "{1} obliterates {0}!",
    },
    DeathMessageTemplate {
        killer: "You smite {0} mightily!",
        victim: "{1} smites you mightily!",
        broadcast: "{1} smites {0} mightily!",
    },
    DeathMessageTemplate {
        killer: "You knock {0} into next Morningthaw!",
        victim: "{1} knocks you into next Morningthaw!",
        broadcast: "{1} knocks {0} into next Morningthaw!",
    },
    DeathMessageTemplate {
        killer: "{0} is utterly destroyed by your attack!",
        victim: "You are utterly destroyed by {1}'s attack!",
        broadcast: "{0} is utterly destroyed by {1}'s attack!",
    },
    DeathMessageTemplate {
        killer: "{0} catches your attack, with dire consequences!",
        victim: "You catch {1}'s attack, with dire consequences!",
        broadcast: "{0} catches {1}'s attack, with dire consequences!",
    },
    DeathMessageTemplate {
        killer: "You slay {0} viciously enough to impart death several times over!",
        victim: "{1} slays you viciously enough to impart death several times over!",
        broadcast: "{1} slays {0} viciously enough to impart death several times over!",
    },
    DeathMessageTemplate {
        killer: "The deadly force of your attack is so strong that {0}'s ancestors feel it!",
        victim: "The deadly force of {1}'s attack is so strong that your ancestors feel it!",
        broadcast: "The deadly force of {1}'s attack is so strong that {0}'s ancestors feel it!",
    },
];

const PKCRITICAL: &[DeathMessageTemplate] = &[DeathMessageTemplate {
    killer: "You send {0} to death so violently that even the lifestone flinches!",
    victim: "{1} sends you to your death so violently that even the lifestone flinches!",
    broadcast: "{1} sends {0} to death so violently that even the lifestone flinches!",
}];

const GENERAL: &[DeathMessageTemplate] = &[
    DeathMessageTemplate {
        killer: "You killed {0}!",
        victim: "You were killed by {1}!",
        broadcast: "{0} was killed by {1}!",
    },
    DeathMessageTemplate {
        killer: "{0} died!",
        victim: "You died!",
        broadcast: "{0} died!",
    },
];

/// ACE DamageType flags; combined or unknown flags select the General death fallback.
pub fn death_message_templates(
    damage_type: u32,
    critical: bool,
) -> &'static [DeathMessageTemplate] {
    if critical {
        return CRITICAL;
    }
    match damage_type {
        0 | 0x80 | 0x1000_0000 => GENERAL,
        0x1 => SLASHING,
        0x2 => PIERCING,
        0x4 => BLUDGEONING,
        0x8 => ICE,
        0x10 => FIRE,
        0x20 => ACID,
        0x40 => LIGHTNING,
        0x400 => VOID,
        _ => &GENERAL[1..2],
    }
}
/// ACE substitutes this fixed text for all three PK-critical messages.
pub fn pk_critical_killer_template() -> DeathMessageTemplate {
    PKCRITICAL[0]
}
/// Self/food/hotspot/unknown deaths use the source General[1] entry.
pub fn unattended_death_template() -> DeathMessageTemplate {
    GENERAL[1]
}
pub fn format_death_message(template: &str, victim: &str, damager: &str) -> String {
    template.replace("{0}", victim).replace("{1}", damager)
}
