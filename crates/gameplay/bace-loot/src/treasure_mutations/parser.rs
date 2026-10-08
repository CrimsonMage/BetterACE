//! Source MutationCache grammar, with bounded fail-closed input handling.
use super::*;
impl MutationScript {
    pub fn parse(source: &str) -> Result<Self, TreasureError> {
        if source.len() > 65536 {
            return Err(TreasureError::Capacity);
        }
        let mut script = Self { mutations: vec![] };
        let mut total = 0u64;
        let mut effects = 0;
        for raw in source.lines() {
            let line = raw.split("//").next().unwrap_or("").trim();
            let lower = line.to_ascii_lowercase();
            if lower.contains("mutation #") {
                continue;
            }
            if lower.contains("tier chances") {
                if script.mutations.len() >= 128 {
                    return Err(TreasureError::Capacity);
                }
                let (_, values) = line.split_once(':').ok_or(TreasureError::Bounds)?;
                let chances = values
                    .split(',')
                    .map(|s| s.trim().parse::<f32>().map_err(|_| TreasureError::Bounds))
                    .collect::<Result<Vec<_>, _>>()?;
                if chances.is_empty()
                    || chances.len() > 16
                    || chances
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                {
                    return Err(TreasureError::Bounds);
                }
                script.mutations.push(Mutation {
                    chances,
                    outcomes: vec![vec![]],
                });
                total = 0;
                continue;
            }
            if lower.contains("- chance") {
                let m = script.mutations.last_mut().ok_or(TreasureError::Bounds)?;
                if total >= 100_000_000 {
                    if m.outcomes.len() >= 64 {
                        return Err(TreasureError::Capacity);
                    }
                    m.outcomes.push(vec![]);
                    total = 0;
                }
                let (_, suffix) = line.split_once(':').ok_or(TreasureError::Bounds)?;
                let suffix = suffix.trim();
                let end = suffix
                    .bytes()
                    .position(|c| !c.is_ascii_digit() && c != b'.')
                    .unwrap_or(suffix.len());
                let number = &suffix[..end];
                total = total
                    .checked_add(decimal_percent(number)?)
                    .ok_or(TreasureError::Bounds)?;
                let lists = m.outcomes.last_mut().ok_or(TreasureError::Bounds)?;
                if lists.len() >= 256 {
                    return Err(TreasureError::Capacity);
                }
                lists.push(EffectList {
                    chance: (total as f64 / 100_000_000.0) as f32,
                    effects: vec![],
                });
                continue;
            }
            if !line.contains('=') {
                continue;
            }
            effects += 1;
            if effects > 4096 {
                return Err(TreasureError::Capacity);
            }
            let effect = parse_effect(line)?;
            let list = script
                .mutations
                .last_mut()
                .and_then(|m| m.outcomes.last_mut())
                .and_then(|o| o.last_mut())
                .ok_or(TreasureError::Bounds)?;
            if list.effects.len() >= 64 {
                return Err(TreasureError::Capacity);
            }
            list.effects.push(effect);
        }
        if script.mutations.is_empty() {
            return Err(TreasureError::Bounds);
        }
        Ok(script)
    }
}
fn decimal_percent(s: &str) -> Result<u64, TreasureError> {
    // Exact decimal accumulation before conversion to source f32 thresholds.
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    if whole.is_empty()
        || fraction.len() > 6
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|c| c.is_ascii_digit())
    {
        return Err(TreasureError::Bounds);
    }
    let whole = whole.parse::<u64>().map_err(|_| TreasureError::Bounds)?;
    let frac = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().map_err(|_| TreasureError::Bounds)?
    };
    if whole > 100 {
        return Err(TreasureError::Bounds);
    }
    Ok(whole * 1_000_000 + frac * 10u64.pow(6 - fraction.len() as u32))
}
fn quality(name: &str) -> Option<Quality> {
    Some(match name {
        "Damage" => Quality::Int(44),
        "ArmorLevel" => Quality::Int(28),
        "EncumbranceVal" => Quality::Int(5),
        "WieldRequirements2" => Quality::Int(270),
        "WieldSkillType2" => Quality::Int(271),
        "WieldDifficulty2" => Quality::Int(272),
        "ArmorModVsSlash" => Quality::Float(13),
        "ArmorModVsPierce" => Quality::Float(14),
        "ArmorModVsBludgeon" => Quality::Float(15),
        "ArmorModVsCold" => Quality::Float(16),
        "ArmorModVsFire" => Quality::Float(17),
        "ArmorModVsAcid" => Quality::Float(18),
        "ArmorModVsElectric" => Quality::Float(19),
        "WeaponSkill" => Quality::Int(48),
        "WieldRequirements" => Quality::Int(158),
        "WieldSkillType" => Quality::Int(159),
        "WieldDifficulty" => Quality::Int(160),
        "ElementalDamageBonus" => Quality::Int(204),
        "DamageVariance" => Quality::Float(22),
        "WeaponDefense" => Quality::Float(29),
        "WeaponOffense" => Quality::Float(62),
        "DamageMod" => Quality::Float(63),
        "ManaConversionMod" => Quality::Float(144),
        "ElementalDamageMod" => Quality::Float(152),
        _ => return None,
    })
}
fn argument(text: &str) -> Result<Argument, TreasureError> {
    let text = text.trim();
    if let Some(q) = quality(text) {
        return Ok(Argument::Quality(q));
    }
    if let Some(value) = match text {
        "MeleeDefense" => Some(6),
        "MissileDefense" => Some(7),
        "MagicDefense" => Some(15),
        _ => None,
    } {
        return Ok(Argument::Int(value));
    }
    if text == "RawSkill" {
        return Ok(Argument::Int(2));
    }
    if text == "Level" {
        return Ok(Argument::Int(7));
    }
    if let Some(body) = text
        .strip_prefix("Random(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (a, b) = body.split_once(',').ok_or(TreasureError::Bounds)?;
        let a = a.trim().parse::<f32>().map_err(|_| TreasureError::Bounds)?;
        let b = b.trim().parse::<f32>().map_err(|_| TreasureError::Bounds)?;
        if !a.is_finite() || !b.is_finite() || a > b {
            return Err(TreasureError::Bounds);
        }
        return Ok(Argument::Random(a, b));
    }
    if text.contains('.') {
        let f = text.parse::<f64>().map_err(|_| TreasureError::Bounds)?;
        if !f.is_finite() {
            return Err(TreasureError::Bounds);
        }
        Ok(Argument::Float(f))
    } else {
        Ok(Argument::Int(
            text.parse().map_err(|_| TreasureError::Bounds)?,
        ))
    }
}
fn parse_effect(line: &str) -> Result<Effect, TreasureError> {
    use Operation::*;
    let (operation, first, second) = if line.contains("+=") {
        if line.contains('*') {
            (AddMultiply, "+=", Some("*"))
        } else if line.contains('/') {
            (AddDivide, "+=", Some("/"))
        } else {
            (Add, "+=", None)
        }
    } else if line.contains("-=") {
        if line.contains('*') {
            (SubtractMultiply, "-=", Some("*"))
        } else if line.contains('/') {
            (SubtractDivide, "-=", Some("/"))
        } else {
            (Subtract, "-=", None)
        }
    } else if line.contains("*=") {
        (Multiply, "*=", None)
    } else if line.contains("/=") {
        (Divide, "/=", None)
    } else if line.contains('+') {
        (AssignAdd, "=", Some("+"))
    } else if line.contains(" - ") {
        (AssignSubtract, "=", Some("-"))
    } else if line.contains('*') {
        (AssignMultiply, "=", Some("*"))
    } else if line.contains('/') {
        (AssignDivide, "=", Some("/"))
    } else if line.contains("(>=") {
        (AtLeastAdd, "(>=", Some("? add : set)"))
    } else if line.contains("(<=") {
        (AtMostSubtract, "(<=", Some("? sub : set)"))
    } else {
        (Assign, "=", None)
    };
    let (left, right) = line.split_once(first).ok_or(TreasureError::Bounds)?;
    let quality = quality(left.trim()).ok_or(TreasureError::Bounds)?;
    let (arg1, arg2) = if let Some(op) = second {
        let (a, b) = right.split_once(op).ok_or(TreasureError::Bounds)?;
        (argument(a)?, Some(argument(b)?))
    } else {
        (argument(right)?, None)
    };
    Ok(Effect {
        quality,
        operation,
        arg1,
        arg2,
    })
}
