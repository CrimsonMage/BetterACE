//! ACE GeneratorProfile destination precedence and requested transforms.
use crate::GeneratorError;
use bace_gameplay_api::*;
use bace_random::{Domain, RandomRoot, RandomStream};

pub(crate) fn generator_stream(
    root: &RandomRoot,
    identity: GeneratorIdentity,
) -> Result<RandomStream, GeneratorError> {
    root.event_stream(identity.random_identity, Domain::Generator)
        .and_then(|s| s.fork(b"entity", u64::from(identity.entity.0)))
        .and_then(|s| s.fork(b"incarnation", identity.incarnation))
        .and_then(|s| s.fork(b"content", identity.content_revision))
        .map_err(|_| GeneratorError::Random)
}

pub fn generator_event_stream(
    root: &RandomRoot,
    intent: &GeneratorSpawnIntent,
    purpose: u32,
) -> Result<RandomStream, GeneratorError> {
    if root.key_version() != intent.random_key_version {
        return Err(GeneratorError::Random);
    }
    root.event_stream(intent.random_identity, Domain::Generator)
        .and_then(|s| s.fork(b"materialization", u64::from(purpose)))
        .map_err(|_| GeneratorError::Random)
}

pub fn generator_destination(
    def: &GeneratorDefinition,
    profile: &GeneratorProfile,
) -> GeneratorDestination {
    let flags = profile.where_create;
    if flags & 4 != 0 {
        let mut loc = GeneratorLocation {
            cell: profile.position.cell.unwrap_or(0),
            origin: profile.position.origin.map(|v| v.unwrap_or(0.0)),
            rotation: profile.position.rotation.map(|v| v.unwrap_or(0.0)),
        };
        if loc.cell == 0 {
            loc.cell = def.location.cell;
            if def.use_rotation_offset {
                let off = rotate(loc.origin, def.location.rotation);
                loc.origin = std::array::from_fn(|i| def.location.origin[i] + off[i]);
                if def.rotation_type == GeneratorRotationType::Relative {
                    loc.rotation = multiply(loc.rotation, def.location.rotation);
                }
            } else {
                loc.origin = std::array::from_fn(|i| {
                    profile.position.origin[i]
                        .map(|v| def.location.origin[i] + v)
                        .unwrap_or(0.0)
                });
            }
        }
        GeneratorDestination::Specific(loc)
    } else if flags & 2 != 0 {
        let mut center = def.location;
        if profile.position.cell.unwrap_or(0) == 0 {
            for (i, v) in center.origin.iter_mut().enumerate() {
                *v += profile.position.origin[i].unwrap_or(0.0);
            }
        }
        center.origin[2] += 0.05;
        GeneratorDestination::Scatter {
            center,
            radius: def.radius,
            attempts: 20,
        }
    } else if flags & 8 != 0 {
        GeneratorDestination::Contain {
            container: def.identity.entity,
        }
    } else if flags & 32 != 0 {
        GeneratorDestination::Shop {
            vendor: def.identity.entity,
        }
    } else {
        GeneratorDestination::Default(def.location)
    }
}
fn rotate(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    // System.Numerics.Vector3.Transform operation order, without normalization.
    let x = q[0] + q[0];
    let y = q[1] + q[1];
    let z = q[2] + q[2];
    let wx = q[3] * x;
    let wy = q[3] * y;
    let wz = q[3] * z;
    let xx = q[0] * x;
    let xy = q[0] * y;
    let xz = q[0] * z;
    let yy = q[1] * y;
    let yz = q[1] * z;
    let zz = q[2] * z;
    [
        v[0] * (1.0 - yy - zz) + v[1] * (xy - wz) + v[2] * (xz + wy),
        v[0] * (xy + wz) + v[1] * (1.0 - xx - zz) + v[2] * (yz - wx),
        v[0] * (xz - wy) + v[1] * (yz + wx) + v[2] * (1.0 - xx - yy),
    ]
}
fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let cx = a[1] * b[2] - a[2] * b[1];
    let cy = a[2] * b[0] - a[0] * b[2];
    let cz = a[0] * b[1] - a[1] * b[0];
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    [
        a[0] * b[3] + b[0] * a[3] + cx,
        a[1] * b[3] + b[1] * a[3] + cy,
        a[2] * b[3] + b[2] * a[3] + cz,
        a[3] * b[3] - dot,
    ]
}
pub fn append_generator_links(
    def: &mut GeneratorDefinition,
    links: &[GeneratorLink],
) -> Result<(), GeneratorError> {
    if def.profiles.len() + links.len() > 256 {
        return Err(GeneratorError::Capacity);
    }
    let Some(first) = def.profiles.first().cloned() else {
        return if links.is_empty() {
            Ok(())
        } else {
            Err(GeneratorError::Definition)
        };
    };
    let mut copy = def.clone();
    for link in links {
        if copy.profiles.iter().any(|p| p.id == link.profile_id) {
            return Err(GeneratorError::Definition);
        }
        copy.profiles.push(GeneratorProfile {
            id: link.profile_id,
            weenie_class_id: link.weenie_class_id,
            probability: first.probability,
            delay: first.delay,
            init_create: first.init_create,
            max_create: first.max_create,
            when_create: first.when_create,
            where_create: first.where_create,
            stack_size: None,
            palette_id: None,
            shade: None,
            position: GeneratorPositionSpec {
                cell: Some(link.location.cell),
                origin: link.location.origin.map(Some),
                rotation: link.location.rotation.map(Some),
            },
        });
        if first.probability == -1.0 {
            copy.initial_count = copy
                .initial_count
                .checked_add(first.init_create)
                .ok_or(GeneratorError::Definition)?;
            copy.maximum_count = copy
                .maximum_count
                .checked_add(first.max_create)
                .ok_or(GeneratorError::Definition)?;
        }
    }
    *def = copy;
    Ok(())
}
