use super::host::Host;
use bace_emotes::NativeEmoteHost;
use bace_entity::{PropertyFamily, PropertyValue};
use bace_gameplay_api::{NpcContext, NpcFailure};
use bace_types::EntityId;
fn property(host: &Host<'_>, actor: Option<EntityId>, family: PropertyFamily, id: u32) -> String {
    actor
        .and_then(|id| host.properties(id))
        .and_then(|p| p.get(family, id))
        .map(|p| match p {
            PropertyValue::String(v) => v.clone(),
            PropertyValue::Int(v) => v.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}
fn time(
    host: &Host<'_>,
    actor: Option<EntityId>,
    name: &str,
    long: bool,
) -> Result<String, NpcFailure> {
    let definition = host
        .state
        .definitions
        .get(&bace_quests::quest_key(name).map_err(|_| NpcFailure::InvalidInput)?);
    let state = actor
        .and_then(|actor| host.state.quests.get(&actor))
        .and_then(|q| q.get(name));
    let now = host
        .state
        .epoch
        .ok_or(NpcFailure::MissingContent)?
        .checked_add(u32::try_from(host.tick / 30).map_err(|_| NpcFailure::InvalidInput)?)
        .ok_or(NpcFailure::InvalidInput)?;
    let eligibility = bace_quests::next_solve(definition, state.as_ref(), now, 1.0)
        .map_err(|_| NpcFailure::InvalidInput)?;
    let (seconds, negative) = match eligibility {
        bace_quests::QuestEligibility::Wait { seconds } => (u64::from(seconds), false),
        bace_quests::QuestEligibility::Ready => (922_337_203_685, true),
        _ => (922_337_203_685, false),
    };
    let values = [
        seconds / 86400,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
    ];
    let mut result = String::new();
    for (index, value) in values.into_iter().enumerate() {
        if value == 0 {
            continue;
        }
        if long {
            if index > 0 && !result.is_empty() {
                result.push_str(if index == 3 { "and " } else { ", " });
            }
            let label = ["day", "hour", "minute", "second"][index];
            result.push_str(&format!(
                "{value} {label}{} ",
                if !negative && value > 1 { "s" } else { "" }
            ));
        } else {
            result.push_str(&format!("{value}{} ", ["d", "h", "m", "s"][index]));
        }
    }
    Ok(result.trim().into())
}
fn replace_ascii_case(source: String, pattern: &str, replacement: &str) -> String {
    if pattern.is_empty() {
        return source;
    }
    let lower = source.to_ascii_lowercase();
    let wanted = pattern.to_ascii_lowercase();
    let mut cursor = 0;
    let mut out = String::new();
    while let Some(offset) = lower[cursor..].find(&wanted) {
        let start = cursor + offset;
        out.push_str(&source[cursor..start]);
        out.push_str(replacement);
        cursor = start + pattern.len();
    }
    out.push_str(&source[cursor..]);
    out
}
pub(super) fn format(
    host: &Host<'_>,
    context: NpcContext,
    message: &str,
    quest: Option<&str>,
) -> Result<String, NpcFailure> {
    if message.len() > 65536 {
        return Err(NpcFailure::Capacity);
    }
    let source = Some(context.source);
    let target = context.target;
    let mut value = message.to_owned();
    for (token, replacement) in [
        ("%n", property(host, source, PropertyFamily::String, 1)),
        ("%mn", property(host, source, PropertyFamily::String, 1)),
        ("%s", property(host, target, PropertyFamily::String, 1)),
        ("%tn", property(host, target, PropertyFamily::String, 1)),
        ("%ml", {
            let v = property(host, source, PropertyFamily::Int, 25);
            if v.is_empty() { "0".into() } else { v }
        }),
        ("%tl", {
            let v = property(host, target, PropertyFamily::Int, 25);
            if target.is_some() && v.is_empty() {
                "0".into()
            } else {
                v
            }
        }),
        ("%mt", property(host, source, PropertyFamily::String, 5)),
        ("%tt", property(host, target, PropertyFamily::String, 5)),
        ("%mh", property(host, source, PropertyFamily::String, 4)),
        ("%th", property(host, target, PropertyFamily::String, 4)),
    ] {
        value = value.replace(token, &replacement);
        if value.len() > 65536 {
            return Err(NpcFailure::Capacity);
        }
    }
    let embedded = message
        .contains('@')
        .then(|| message.split('@').next().unwrap_or(""));
    let name = embedded
        .filter(|n| !n.trim().is_empty())
        .or(quest)
        .unwrap_or("");
    value = replace_ascii_case(
        value,
        &format!("{name}@%tqt"),
        "You may complete this quest again in %tqt.",
    );
    if value.contains("%CDtime") {
        value = replace_ascii_case(value, &format!("{name}@"), "");
    }
    let active = quest.is_some_and(|q| !q.trim().is_empty());
    if target
        .and_then(|id| host.facts(id))
        .is_some_and(|f| f.player)
    {
        let next = if active {
            time(host, target, name, false)?
        } else {
            String::new()
        };
        value = value.replace("%tqt", &next).replace("%CDtime", &next);
        // Fellowship data is resolved by its owner; absent membership is empty.
        let fellowship = target
            .and_then(|actor| host.fellowships.members.get(&actor))
            .and_then(|id| host.fellowships.groups.get(id));
        value = value.replace("%tf", fellowship.map_or("", |f| f.name()));
        if fellowship.is_none() {
            value = value.replace("%fqt", "");
        } else if value.contains("%fqt") {
            return Err(NpcFailure::Unsupported);
        }
        let max = host
            .state
            .definitions
            .get(&bace_quests::quest_key(name).unwrap_or_default())
            .map_or(0, |d| d.maximum_solves);
        let count = target
            .and_then(|id| host.state.quests.get(&id))
            .and_then(|q| q.get(name))
            .map_or(0, |q| q.completions);
        value = value
            .replace(
                "%tqm",
                &if active {
                    max.to_string()
                } else {
                    String::new()
                },
            )
            .replace(
                "%tqc",
                &if active {
                    count.to_string()
                } else {
                    String::new()
                },
            );
    }
    if host.facts(context.source).is_some_and(|f| f.creature) {
        let short = if active {
            time(host, source, name, false)?
        } else {
            String::new()
        };
        let long = if active {
            time(host, source, name, true)?
        } else {
            String::new()
        };
        let count = host
            .state
            .quests
            .get(&context.source)
            .and_then(|q| q.get(name))
            .map_or(0, |q| q.completions);
        value = value
            .replace("%mqt", &short)
            .replace("%mxqt", &long)
            .replace(
                "%mqc",
                &if active {
                    count.to_string()
                } else {
                    String::new()
                },
            );
    }
    if value.len() > 65536 {
        return Err(NpcFailure::Capacity);
    }
    Ok(value)
}
