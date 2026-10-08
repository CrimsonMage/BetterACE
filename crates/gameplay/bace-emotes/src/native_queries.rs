use crate::native::{NativeEmoteHost, NativeError, NativeProgram, NativeTrigger, text};
use bace_content::EmoteAction;
use bace_gameplay_api::*;
pub(crate) fn quest_subject(
    code: u32,
    context: NpcContext,
    host: &impl NativeEmoteHost,
) -> NpcSubject {
    if matches!(code, 79..=86 | 104 | 105 | 108 | 109) {
        NpcSubject::Source
    } else if context
        .target
        .and_then(|id| host.facts(id))
        .is_some_and(|f| f.creature)
    {
        NpcSubject::Target
    } else {
        NpcSubject::Source
    }
}
fn has(
    program: &NativeProgram,
    category: u32,
    key: String,
    host: &mut impl NativeEmoteHost,
) -> Result<bool, NativeError> {
    let draw = host.draw()?;
    Ok(program
        .select(
            &NativeTrigger {
                category,
                quest: Some(key),
                random: true,
                ..Default::default()
            },
            Some(draw),
        )?
        .is_some())
}
fn integer(a: &EmoteAction) -> Result<u32, NativeError> {
    u32::try_from(a.stat.ok_or(NativeError::InvalidContent)?)
        .map_err(|_| NativeError::InvalidContent)
}
pub(crate) fn query(
    program: &NativeProgram,
    a: &EmoteAction,
    context: NpcContext,
    host: &mut impl NativeEmoteHost,
) -> Result<Option<NpcCompletion>, NativeError> {
    let code = a.r#type;
    let family = match code {
        35 => Some(NpcPropertyFamily::Bool),
        36 => Some(NpcPropertyFamily::Int),
        37 => Some(NpcPropertyFamily::Float),
        38 => Some(NpcPropertyFamily::String),
        39 => Some(NpcPropertyFamily::Attribute),
        40 => Some(NpcPropertyFamily::RawAttribute),
        41 => Some(NpcPropertyFamily::Vital),
        42 => Some(NpcPropertyFamily::RawVital),
        43 => Some(NpcPropertyFamily::Skill),
        44 => Some(NpcPropertyFamily::RawSkill),
        45 | 46 => Some(NpcPropertyFamily::SkillAdvancement),
        114 => Some(NpcPropertyFamily::Int64),
        _ => None,
    };
    if let Some(family) = family {
        let target = context.target.and_then(|id| host.facts(id));
        if target.is_none() || matches!(code, 39..=46) && target.is_some_and(|f| !f.creature) {
            return Ok(Some(NpcCompletion::Applied { post_delay: 0.0 }));
        }
        let value = host.query(
            context,
            NpcQuery::Property {
                subject: NpcSubject::Target,
                family,
                stat: integer(a)?,
            },
        )?;
        if value == NpcQueryValue::Absent && has(program, 29, text(a), host)? {
            return Ok(Some(NpcCompletion::Branch { category: 29 }));
        }
        let success = match value {
            NpcQueryValue::Value(NpcValue::Bool(v)) => v,
            NpcQueryValue::Value(NpcValue::Int(v)) if matches!(code, 45 | 46) => {
                if code == 45 {
                    v >= 2
                } else {
                    v == 3
                }
            }
            NpcQueryValue::Value(NpcValue::Int(v)) => {
                v >= a.min.unwrap_or(i32::MIN) && v <= a.max.unwrap_or(i32::MAX)
            }
            NpcQueryValue::Value(NpcValue::Unsigned(v)) if matches!(code, 45 | 46) => {
                if code == 45 { v >= 2 } else { v == 3 }
            }
            NpcQueryValue::Value(NpcValue::Unsigned(v)) => {
                i64::from(v) >= i64::from(a.min.unwrap_or(i32::MIN))
                    && i64::from(v) <= i64::from(a.max.unwrap_or(i32::MAX))
            }
            NpcQueryValue::Value(NpcValue::Int64(v)) => {
                v >= a.min64.unwrap_or(i64::MIN) && v <= a.max64.unwrap_or(i64::MAX)
            }
            NpcQueryValue::Value(NpcValue::Float(v)) => {
                v >= a.min_dbl.unwrap_or(f64::MIN) && v <= a.max_dbl.unwrap_or(f64::MAX)
            }
            NpcQueryValue::Value(NpcValue::String(v)) => Some(&v) == a.test_string.as_ref(),
            NpcQueryValue::Absent => match code {
                36 => 0 >= a.min.unwrap_or(i32::MIN) && 0 <= a.max.unwrap_or(i32::MAX),
                37 => 0.0 >= a.min_dbl.unwrap_or(f64::MIN) && 0.0 <= a.max_dbl.unwrap_or(f64::MAX),
                114 => 0 >= a.min64.unwrap_or(i64::MIN) && 0 <= a.max64.unwrap_or(i64::MAX),
                _ => false,
            },
            _ => return Err(NativeError::InvalidContent),
        };
        return Ok(Some(NpcCompletion::Branch {
            category: if success { 22 } else { 23 },
        }));
    }
    let request = match code {
        21 | 80 | 58 => NpcQuery::Quest {
            subject: if code == 58 {
                NpcSubject::Fellowship
            } else {
                quest_subject(code, context, host)
            },
            name: text(a),
            check: NpcQuestCheck::HasAndCannotSolve,
        },
        30 | 82 => NpcQuery::Quest {
            subject: quest_subject(code, context, host),
            name: text(a),
            check: NpcQuestCheck::Solves {
                minimum: a.min,
                maximum: a.max,
            },
        },
        102..=105 => NpcQuery::Quest {
            subject: quest_subject(code, context, host),
            name: text(a),
            check: NpcQuestCheck::Bits {
                mask: a.amount.unwrap_or(0),
                on: code == 102 || code == 104,
            },
        },
        51 => NpcQuery::Event(text(a)),
        59 => NpcQuery::FellowCount,
        71 => NpcQuery::TitleCount,
        121 => NpcQuery::ContractsFull,
        76 => NpcQuery::ItemCount {
            template: a.weenie_class_id.unwrap_or(0),
        },
        89 => NpcQuery::PackSpace {
            containers: a.amount.unwrap_or(1) > 10000,
        },
        _ => return Ok(None),
    };
    let player = context
        .target
        .and_then(|id| host.facts(id))
        .is_some_and(|f| f.player);
    if matches!(code, 58 | 59 | 71 | 76 | 89) && !player {
        return Ok(Some(NpcCompletion::Applied { post_delay: 0.0 }));
    }
    if code == 121 && !player {
        return Ok(Some(NpcCompletion::Branch { category: 23 }));
    }
    let value = host.query(context, request)?;
    if value == NpcQueryValue::NoFellow {
        return Ok(Some(NpcCompletion::Branch {
            category: if code == 58 {
                30
            } else if has(program, 31, text(a), host)? {
                31
            } else {
                34
            },
        }));
    }
    let success = match value {
        NpcQueryValue::Value(NpcValue::Bool(v)) => v,
        NpcQueryValue::Value(NpcValue::Int64(v)) => match code {
            59 | 71 => {
                v >= i64::from(a.min.unwrap_or(i32::MIN))
                    && v <= i64::from(a.max.unwrap_or(i32::MAX))
            }
            76 => v >= i64::from(a.stack_size.unwrap_or(1)),
            89 => {
                let required = a.amount.unwrap_or(1);
                v >= i64::from(if required > 10000 {
                    required - 10000
                } else {
                    required
                })
            }
            _ => return Err(NativeError::InvalidContent),
        },
        NpcQueryValue::Absent => false,
        _ => return Err(NativeError::InvalidContent),
    };
    let (yes, no) = match code {
        21 | 80 | 58 | 30 | 82 | 102..=105 => (12, 13),
        51 => (27, 28),
        59 => (33, 34),
        71 => (35, 36),
        _ => (22, 23),
    };
    Ok(Some(NpcCompletion::Branch {
        category: if success { yes } else { no },
    }))
}
