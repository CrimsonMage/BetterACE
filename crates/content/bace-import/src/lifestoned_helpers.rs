use crate::ImportError;
use serde_json::{Map, Value};

pub(crate) type Object = Map<String, Value>;

pub(crate) fn object(value: Value, name: &str) -> Result<Object, ImportError> {
    match value {
        Value::Object(value) => Ok(value),
        _ => Err(ImportError::Unsupported(format!(
            "{name} must be an object"
        ))),
    }
}
pub(crate) fn array(value: Value, name: &str) -> Result<Vec<Value>, ImportError> {
    match value {
        Value::Array(value) => Ok(value),
        Value::Null => Ok(vec![]),
        _ => Err(ImportError::Unsupported(format!("{name} must be an array"))),
    }
}
pub(crate) fn take(value: &mut Object, name: &str) -> Result<Value, ImportError> {
    value
        .remove(name)
        .ok_or_else(|| ImportError::Unsupported(format!("missing {name}")))
}
pub(crate) fn optional(value: &mut Object, name: &str) -> Option<Value> {
    value.remove(name).filter(|v| !v.is_null())
}
pub(crate) fn done(value: &Object, name: &str) -> Result<(), ImportError> {
    if !value.is_empty() {
        return Err(ImportError::Unsupported(format!(
            "{name}: {}",
            value.keys().cloned().collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(())
}
pub(crate) fn rename(value: &mut Object, fields: &[(&str, &str)]) -> Object {
    fields
        .iter()
        .filter_map(|(old, new)| optional(value, old).map(|v| ((*new).into(), v)))
        .collect()
}
pub(crate) fn flag(value: Value) -> Result<Value, ImportError> {
    match value {
        Value::Bool(_) => Ok(value),
        Value::Number(n) if n.as_u64() == Some(0) => Ok(false.into()),
        Value::Number(n) if n.as_u64() == Some(1) => Ok(true.into()),
        _ => Err(ImportError::Unsupported(
            "boolean value must be 0 or 1".into(),
        )),
    }
}

pub(crate) fn frame(value: Value, position_names: bool) -> Result<Object, ImportError> {
    let mut frame = object(value, "frame")?;
    let mut origin = object(take(&mut frame, "origin")?, "origin")?;
    let mut angles = object(take(&mut frame, "angles")?, "angles")?;
    let mut result = if position_names {
        rename(
            &mut origin,
            &[
                ("x", "position_x"),
                ("y", "position_y"),
                ("z", "position_z"),
            ],
        )
    } else {
        rename(
            &mut origin,
            &[("x", "origin_x"), ("y", "origin_y"), ("z", "origin_z")],
        )
    };
    result.extend(if position_names {
        rename(
            &mut angles,
            &[
                ("w", "rotation_w"),
                ("x", "rotation_x"),
                ("y", "rotation_y"),
                ("z", "rotation_z"),
            ],
        )
    } else {
        rename(
            &mut angles,
            &[
                ("w", "angles_w"),
                ("x", "angles_x"),
                ("y", "angles_y"),
                ("z", "angles_z"),
            ],
        )
    });
    done(&frame, "frame")?;
    done(&origin, "origin")?;
    done(&angles, "angles")?;
    Ok(result)
}

pub(crate) fn creation(value: Value, nullable: bool) -> Result<Object, ImportError> {
    let mut value = object(value, "create item")?;
    let mut result = rename(
        &mut value,
        &[
            ("wcid", "weenie_class_id"),
            ("palette", "palette"),
            ("shade", "shade"),
            ("destination", "destination_type"),
            ("stack_size", "stack_size"),
        ],
    );
    if let Some(v) = optional(&mut value, "try_to_bond") {
        result.insert("try_to_bond".into(), flag(v)?);
    }
    if !nullable {
        result.insert("database_record_id".into(), 0.into());
    }
    done(&value, "create item")?;
    Ok(result)
}

pub(crate) fn key_value(value: Value, label: &str) -> Result<(Value, Value), ImportError> {
    let mut object = object(value, label)?;
    let key = take(&mut object, "key")?;
    let value = take(&mut object, "value")?;
    done(&object, label)?;
    Ok((key, value))
}
