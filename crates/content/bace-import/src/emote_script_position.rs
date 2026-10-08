//! EmoteScript movement shorthand; explicit fields remain the lossless representation.
use crate::emote_script_schema::parse_value;
use serde_json::Value;

pub(crate) fn fields(kind: &str, text: &str) -> Result<Vec<(String, Value)>, String> {
    let mut result = Vec::new();
    let mut rest = text.trim();
    if kind == "Position" {
        let (cell, tail) = rest
            .split_once(char::is_whitespace)
            .ok_or("Position requires cell and frame")?;
        result.push(("obj_cell_id".into(), parse_value("UInt32", cell)?));
        rest = tail.trim();
    }
    if kind != "Angles" {
        let end = rest.find(']').filter(|_| rest.starts_with('['));
        let (origin, tail) = if let Some(end) = end {
            (&rest[1..end], rest[end + 1..].trim())
        } else if kind == "OriginAngles" {
            (rest, "")
        } else {
            return Err("Position requires [x y z] followed by w x y z".into());
        };
        let coordinates: Vec<_> = origin.split_whitespace().collect();
        if coordinates.len() != 3 {
            return Err("Origin requires three coordinates".into());
        }
        for (key, number) in ["origin_x", "origin_y", "origin_z"]
            .into_iter()
            .zip(coordinates)
        {
            result.push((key.into(), parse_value("Float", number)?));
        }
        rest = tail;
        if rest.is_empty() && kind == "OriginAngles" {
            return Ok(result);
        }
    }
    let numbers: Vec<_> = rest.split_whitespace().collect();
    let quaternion = if numbers.len() == 4 {
        numbers
            .into_iter()
            .map(|n| parse_value("Float", n))
            .collect::<Result<Vec<_>, _>>()?
    } else if kind == "Angles" && numbers.len() == 1 {
        let degrees = match rest.to_ascii_lowercase().as_str() {
            "n" | "north" => 0.0_f32,
            "nw" | "northwest" => 45.0,
            "w" | "west" => 90.0,
            "sw" | "southwest" => 135.0,
            "s" | "south" => 180.0,
            "se" | "southeast" => 225.0,
            "e" | "east" => 270.0,
            "ne" | "northeast" => 315.0,
            _ => rest.parse::<f32>().map_err(|_| "Unknown heading")?,
        };
        if !degrees.is_finite() || degrees.abs() > 360_000.0 {
            return Err("Heading exceeds finite bounds".into());
        }
        let (z, w) = (degrees * std::f32::consts::PI / 180.0 * 0.5).sin_cos();
        vec![
            Value::from(w),
            Value::from(0.0),
            Value::from(0.0),
            Value::from(z),
        ]
    } else {
        return Err("Orientation requires w x y z, or a Turn heading".into());
    };
    result.extend(
        ["angles_w", "angles_x", "angles_y", "angles_z"]
            .into_iter()
            .zip(quaternion)
            .map(|(k, v)| (k.into(), v)),
    );
    Ok(result)
}
