use crate::native::{NativeEmoteHost, NativeError, NativeProgram, text};
use bace_content::{Emote, EmoteAction};
use bace_gameplay_api::*;
use bace_geometry::Vec3;
use bace_types::CellId;
pub(crate) fn branches(code: u32) -> bool {
    matches!(code,20|21|30|35..=46|51|58..=60|67|71|75|76|79|80|82|89|102..=105|114|121)
}
pub(crate) fn known(code: u32) -> bool {
    code <= 121 || code == 9001
}
pub fn source_noop(code: u32) -> bool {
    matches!(code, 0 | 91..=98 | 100 | 111)
}
fn stat(a: &EmoteAction) -> Result<u32, NativeError> {
    u32::try_from(a.stat.ok_or(NativeError::InvalidContent)?)
        .map_err(|_| NativeError::InvalidContent)
}
fn amount(a: &EmoteAction) -> i64 {
    a.amount64.or(a.amount.map(i64::from)).unwrap_or(0)
}
fn rotation(a: &EmoteAction) -> [f32; 4] {
    [
        a.angles_w.unwrap_or(1.0),
        a.angles_x.unwrap_or(0.0),
        a.angles_y.unwrap_or(0.0),
        a.angles_z.unwrap_or(0.0),
    ]
}
fn destination(a: &EmoteAction, required: bool) -> Result<NpcDestination, NativeError> {
    if required
        && [
            a.origin_x, a.origin_y, a.origin_z, a.angles_w, a.angles_x, a.angles_y, a.angles_z,
        ]
        .iter()
        .any(Option::is_none)
    {
        return Err(NativeError::InvalidContent);
    }
    let position = Vec3::new(
        a.origin_x.unwrap_or(0.0),
        a.origin_y.unwrap_or(0.0),
        a.origin_z.unwrap_or(0.0),
    );
    let rotation = rotation(a);
    if !position.is_finite() || rotation.iter().any(|v| !v.is_finite()) {
        return Err(NativeError::InvalidContent);
    }
    Ok(NpcDestination {
        cell: a.obj_cell_id.map(CellId),
        position,
        rotation,
        relative: a.obj_cell_id == Some(0),
    })
}
pub(crate) fn execute(
    program: &NativeProgram,
    set: &Emote,
    a: &EmoteAction,
    context: NpcContext,
    host: &mut impl NativeEmoteHost,
) -> Result<(NpcCompletion, bool), NativeError> {
    if source_noop(a.r#type) {
        return Ok((NpcCompletion::Applied { post_delay: 0.0 }, true));
    }
    if let Some(result) = crate::native_queries::query(program, a, context, host)? {
        return Ok((result, false));
    }
    let player = context
        .target
        .and_then(|id| host.facts(id))
        .is_some_and(|f| f.player);
    let creature = host.facts(context.source).is_some_and(|f| f.creature);
    let player_only = matches!(a.r#type,2|3|10|13|18|27..=29|34|47..=50|53|56|60..=66|68|69|74|75|90|99|101|110|112|113|115|118..=120|9001);
    if player_only && !player {
        return Ok((NpcCompletion::Applied { post_delay: 0.0 }, false));
    }
    if matches!(a.r#type, 4 | 6 | 11 | 12 | 14 | 73 | 78 | 87) && !creature {
        return Ok((NpcCompletion::Applied { post_delay: 0.0 }, false));
    }
    let operation = match a.r#type {
        1 | 8 | 10 | 13 | 16..=18 | 25 | 26 | 64 | 65 | 68 => {
            let kind = match a.r#type {
                1 => NpcTextKind::Act,
                8 => NpcTextKind::Say,
                10 => NpcTextKind::Tell,
                13 | 18 => NpcTextKind::Direct,
                16 => NpcTextKind::World,
                17 => NpcTextKind::Local,
                25 => NpcTextKind::Log,
                26 => NpcTextKind::Admin,
                64 => NpcTextKind::FellowTell,
                65 => NpcTextKind::FellowBroadcast,
                68 => NpcTextKind::Popup,
                _ => unreachable!(),
            };
            let text = if a.r#type == 68 {
                text(a)
            } else {
                host.text(context, &text(a), set.quest.as_deref())?
            };
            if text.len() > 65536 {
                return Err(NativeError::Capacity);
            }
            NpcOperation::Text {
                kind,
                text,
                extent: a.extent,
            }
        }
        2 | 27..=29 | 34 | 47..=50 | 62 | 90 | 110 | 112 | 113 | 9001 => {
            let kind = match a.r#type {
                2 => NpcRewardKind::Experience,
                27 => NpcRewardKind::TeachSpell,
                28 => NpcRewardKind::SkillExperience,
                29 => NpcRewardKind::SkillPoints,
                34 => NpcRewardKind::Title,
                47 => NpcRewardKind::TrainingCredits,
                48 => NpcRewardKind::Vitae,
                49 => NpcRewardKind::LevelExperience,
                50 => NpcRewardKind::LevelSkillExperience,
                62 => NpcRewardKind::NoShareExperience,
                90 => NpcRewardKind::RemoveVitae,
                110 => NpcRewardKind::UntrainSkill,
                112 => NpcRewardKind::SpendLuminance,
                113 => NpcRewardKind::Luminance,
                9001 => NpcRewardKind::Enlightenment,
                _ => unreachable!(),
            };
            let amount = match a.r#type {
                112 | 113 => a.amount64.or(a.hero_xp64).unwrap_or(0),
                48 => i64::from(a.amount.unwrap_or(5)),
                27 => i64::from(a.spell_id.ok_or(NativeError::InvalidContent)?),
                _ => amount(a),
            };
            NpcOperation::Reward {
                kind,
                amount,
                stat: a
                    .stat
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| NativeError::InvalidContent)?,
                percent: a.percent.unwrap_or(0.0),
                minimum: a.min64.or(a.min.map(i64::from)).unwrap_or(0),
                maximum: a.max64.or(a.max.map(i64::from)).unwrap_or(0),
            }
        }
        3 => NpcOperation::Give {
            template: a.weenie_class_id.ok_or(NativeError::InvalidContent)?,
            count: a.stack_size.unwrap_or(1).max(1) as u32,
            palette: a.palette.unwrap_or(0),
            shade: a.shade.unwrap_or(0.0),
        },
        74 => {
            let count = a.stack_size.unwrap_or(1);
            let template = a.weenie_class_id.unwrap_or(0);
            if template == 0 || count == 0 || count < -1 {
                return Err(NativeError::InvalidContent);
            }
            NpcOperation::Take {
                template,
                count: if count == -1 {
                    None
                } else {
                    Some(count as u32)
                },
            }
        }
        20 | 22 | 31..=33 | 60 | 61 | 70 | 79 | 81 | 83..=86 | 106..=109 => {
            let subject = if matches!(a.r#type, 60 | 61) {
                NpcSubject::Fellowship
            } else {
                crate::native_queries::quest_subject(a.r#type, context, host)
            };
            let mutation = match a.r#type {
                20 | 60 | 79 => NpcQuestMutation::Update,
                22 | 61 | 81 => NpcQuestMutation::Stamp,
                31 | 83 => NpcQuestMutation::Erase,
                32 | 84 => NpcQuestMutation::Decrement(a.amount.unwrap_or(1)),
                33 | 85 => NpcQuestMutation::Increment(a.amount.unwrap_or(1)),
                70 | 86 => {
                    NpcQuestMutation::Completions(a.amount.ok_or(NativeError::InvalidContent)?)
                }
                106..=109 => NpcQuestMutation::Bits {
                    mask: a.amount.ok_or(NativeError::InvalidContent)?,
                    on: a.r#type == 106 || a.r#type == 108,
                },
                _ => unreachable!(),
            };
            NpcOperation::Quest {
                subject,
                name: text(a),
                mutation,
            }
        }
        53 | 54 | 55 | 69 | 115 | 118 => {
            let mutation = match a.r#type {
                53 => NpcPropertyMutation::Set {
                    family: NpcPropertyFamily::Int,
                    value: a.amount.map(NpcValue::Int),
                },
                54 => NpcPropertyMutation::AddInt(a.amount.unwrap_or(1)),
                55 => NpcPropertyMutation::AddInt(
                    a.amount
                        .unwrap_or(1)
                        .checked_neg()
                        .ok_or(NativeError::InvalidContent)?,
                ),
                69 => NpcPropertyMutation::Set {
                    family: NpcPropertyFamily::Bool,
                    value: Some(NpcValue::Bool(a.amount != Some(0))),
                },
                115 => NpcPropertyMutation::Set {
                    family: NpcPropertyFamily::Int64,
                    value: a.amount64.map(NpcValue::Int64),
                },
                118 => NpcPropertyMutation::Set {
                    family: NpcPropertyFamily::Float,
                    value: a.percent.map(NpcValue::Float),
                },
                _ => unreachable!(),
            };
            NpcOperation::Property {
                subject: NpcSubject::Target,
                stat: stat(a)?,
                mutation,
            }
        }
        5 | 52 => NpcOperation::Motion {
            target: a.r#type == 52,
            motion: a.motion.ok_or(NativeError::InvalidContent)?,
            extent: a.extent,
            style: set.style,
            substyle: set.substyle,
        },
        4 | 6 | 87 => NpcOperation::Move {
            home: a.r#type == 4,
            absolute: a.r#type == 87,
            destination: destination(a, false)?,
            extent: a.extent,
        },
        11 | 12 => NpcOperation::Turn {
            target: a.r#type == 12,
            rotation: rotation(a),
        },
        7 => NpcOperation::Particle {
            script: a.p_script.ok_or(NativeError::InvalidContent)?,
            extent: a.extent,
        },
        9 => NpcOperation::Sound(a.sound.ok_or(NativeError::InvalidContent)?),
        14 | 19 | 73 => NpcOperation::Cast {
            spell: u32::try_from(a.spell_id.ok_or(NativeError::InvalidContent)?)
                .map_err(|_| NativeError::InvalidContent)?,
            instant: a.r#type != 14,
            pet_owner: a.r#type == 73,
        },
        15 => NpcOperation::Activate,
        23 | 24 => NpcOperation::Event {
            name: text(a),
            start: a.r#type == 23,
        },
        56 => NpcOperation::Treasure {
            tier: a.wealth_rating.unwrap_or(1),
            category: a.treasure_type.unwrap_or(0),
            class: a.treasure_class.unwrap_or(0),
        },
        57 => NpcOperation::ResetHome,
        63 => NpcOperation::Sanctuary(destination(a, true)?),
        66 => NpcOperation::LockFellow {
            quest: set.quest.clone(),
        },
        67 => return Ok((NpcCompletion::Branch { category: 32 }, false)),
        72 => NpcOperation::Generate,
        75 => NpcOperation::Confirm {
            key: text(a),
            text: host.text(
                context,
                a.test_string.as_deref().unwrap_or_default(),
                set.quest.as_deref(),
            )?,
        },
        77 => NpcOperation::DeleteSelf,
        78 => NpcOperation::KillSelf,
        88 => NpcOperation::Signal(text(a)),
        99 => NpcOperation::TeleportTarget(destination(a, true)?),
        101 => NpcOperation::Barber,
        116 | 117 => NpcOperation::OpenSelf(a.r#type == 116),
        119 | 120 => {
            let id = stat(a)?;
            if id == 0 {
                return Ok((NpcCompletion::Applied { post_delay: 0.0 }, false));
            }
            NpcOperation::Contract {
                id,
                add: a.r#type == 119,
            }
        }
        _ => return Err(NativeError::InvalidContent),
    };
    Ok((host.execute(context, operation)?, false))
}
