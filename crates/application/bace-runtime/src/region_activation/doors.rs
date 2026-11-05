//! Pinned GDLE Door.cpp Motion_On/Off and AnimHooks.cpp EtherealHook data.
//! All animation records are read on the region asset worker, never on a tick.
use super::*;
use bace_dat::{AnimationHookPayload, MotionData};
use bace_geometry::{Aabb, Vec3};
use bace_simulation::{
    PreparedDoor, PreparedDoorAnimation, PreparedDoorHook, PreparedGeneratorRegion,
};
use bace_types::{CellId, EntityId};
use bace_world::DoorCollider;

impl VerifiedRegionAssets {
    pub(crate) fn prepare_doors(
        &mut self,
        region: &PreparedRegionActivation,
        roots: &PreparedGeneratorRegion,
    ) -> Result<Vec<(EntityId, PreparedDoor)>, String> {
        let mut doors = Vec::new();
        for instance in region
            .content
            .instances
            .iter()
            .filter(|i| !i.source.is_link_child && i.template.weenie_type == 19)
        {
            let id = EntityId(instance.source.guid);
            let root = roots
                .roots
                .iter()
                .find(|root| root.entity == id)
                .ok_or_else(|| format!("authored door {:#010x} has no admitted root", id.0))?;
            let template = instance.template.as_ref();
            let motion_id = template
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == 2 && p.value != 0)
                .ok_or("authored door missing motion table")?
                .value;
            let mut budget = 16 * 1024 * 1024usize;
            let table = self.cached_motion(motion_id, &mut budget)?;
            let style = table.default_style;
            let open = self.door_animation(&table, style, 0x4000_000b, true, &mut budget)?;
            let close = self.door_animation(&table, style, 0x4000_000c, false, &mut budget)?;
            let position = Vec3::new(
                root.location.origin[0],
                root.location.origin[1],
                root.location.origin[2],
            );
            let radius = root.shape.horizontal_radius();
            let height = root.shape.height();
            if !radius.is_finite() || radius <= 0.0 || !height.is_finite() || height <= 0.0 {
                return Err("authored door collision bounds invalid".into());
            }
            let bounds = Aabb::new(
                position + Vec3::new(-radius, -radius, 0.0),
                position + Vec3::new(radius, radius, height),
            )
            .ok_or("authored door collision bounds invalid")?;
            let float = |property: u32| {
                template
                    .properties
                    .floats
                    .iter()
                    .find(|p| p.id == property)
                    .map(|p| p.value)
            };
            let reset_interval_ticks = float(11)
                .map(|seconds| seconds_to_ticks(seconds, true))
                .transpose()?;
            let use_radius = float(54).unwrap_or(0.0) as f32;
            if !use_radius.is_finite() || !(0.0..=192.0).contains(&use_radius) {
                return Err("authored door use radius invalid".into());
            }
            let initially_locked = template
                .properties
                .bools
                .iter()
                .find(|p| p.id == 3)
                .is_some_and(|p| p.value);
            doors.push((
                id,
                PreparedDoor {
                    collider: DoorCollider {
                        cell: CellId(root.location.cell),
                        position,
                        bounds,
                        solid: true,
                    },
                    style: style as u16,
                    initially_open: false,
                    initially_locked,
                    reset_interval_ticks,
                    use_radius,
                    open,
                    close,
                },
            ));
        }
        Ok(doors)
    }

    fn door_animation(
        &mut self,
        table: &MotionTable,
        style: u32,
        motion: u32,
        expected_ethereal: bool,
        budget: &mut usize,
    ) -> Result<PreparedDoorAnimation, String> {
        // GDLE MotionTable::GetObjectSequence uses a link from the current
        // substate to Motion_On/Off. The cycle record is a zero-rate held pose.
        let from = if expected_ethereal {
            0x4000_000c
        } else {
            0x4000_000b
        };
        let key = style.wrapping_shl(16) | (from & 0x00ff_ffff);
        let data = table
            .links
            .get(&key)
            .and_then(|targets| targets.get(&motion))
            .ok_or_else(|| format!("authored door missing {from:#010x}->{motion:#010x} link"))?;
        prepare_animation(self, data, expected_ethereal, budget)
    }
}

fn prepare_animation(
    assets: &mut VerifiedRegionAssets,
    data: &MotionData,
    expected_ethereal: bool,
    budget: &mut usize,
) -> Result<PreparedDoorAnimation, String> {
    let mut elapsed = 0.0_f64;
    let mut hooks = Vec::new();
    for segment in &data.animations {
        let animation = assets.cached_animation(segment.animation_id, budget)?;
        let high = if segment.high_frame == -1 {
            animation.num_frames as i32 - 1
        } else {
            segment.high_frame
        };
        if animation.id != segment.animation_id
            || segment.low_frame < 0
            || high < segment.low_frame
            || high as u32 >= animation.num_frames
            || segment.framerate == 0.0
            || !segment.framerate.is_finite()
        {
            return Err("authored door animation range/rate invalid".into());
        }
        let rate = f64::from(segment.framerate.abs());
        let reverse = segment.framerate < 0.0;
        for frame in segment.low_frame..=high {
            let frame = if reverse {
                high - (frame - segment.low_frame)
            } else {
                frame
            };
            for hook in &animation.frames[frame as usize].hooks {
                if hook.kind != 6
                    || (hook.direction != 0 && hook.direction != if reverse { -1 } else { 1 })
                {
                    continue;
                }
                let AnimationHookPayload::State(state) = hook.payload else {
                    return Err("authored door ethereal hook payload invalid".into());
                };
                if state != 0 && state != 1 {
                    return Err("authored door ethereal hook state invalid".into());
                }
                let frame_offset = if reverse {
                    high - frame + 1
                } else {
                    frame - segment.low_frame + 1
                };
                let at = elapsed + f64::from(frame_offset) / rate;
                hooks.push(PreparedDoorHook {
                    offset_ticks: seconds_to_ticks(at, false)?,
                    ethereal: state != 0,
                });
            }
        }
        elapsed += f64::from(high - segment.low_frame + 1) / rate;
    }
    if hooks
        .last()
        .is_none_or(|hook| hook.ethereal != expected_ethereal)
    {
        return Err("authored door motion lacks final ethereal hook".into());
    }
    let duration_ticks = seconds_to_ticks(elapsed, false)?.max(1);
    Ok(PreparedDoorAnimation {
        duration_ticks,
        hooks,
    })
}

fn seconds_to_ticks(seconds: f64, allow_zero: bool) -> Result<u64, String> {
    if !seconds.is_finite() || seconds < 0.0 || (!allow_zero && seconds == 0.0) {
        return Err("authored door duration invalid".into());
    }
    let exact = seconds * 30.0;
    // Integer frame boundaries such as 31/30 must not slip by one tick from
    // floating-point division before the 30 Hz ceiling is applied.
    let ticks = if (exact - exact.round()).abs() < 1e-9 {
        exact.round()
    } else {
        exact.ceil()
    };
    if ticks > 30.0 * 3600.0 {
        return Err("authored door duration exceeds one hour".into());
    }
    Ok(ticks as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires approved retail DATs"]
    fn approved_retail_door_motion_hooks_prepare() {
        let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
        let mut assets = VerifiedRegionAssets::open(&RegionAssetManifest {
            portal: directory.join("client_portal.dat"),
            cell: directory.join("client_cell_1.dat"),
            portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
                .into(),
            cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
            store: None,
        })
        .unwrap();
        let mut budget = 16 * 1024 * 1024;
        let table = assets.cached_motion(0x09000086, &mut budget).unwrap();
        let style = table.default_style;
        let open = assets
            .door_animation(&table, style, 0x4000000b, true, &mut budget)
            .unwrap();
        let close = assets
            .door_animation(&table, style, 0x4000000c, false, &mut budget)
            .unwrap();
        assert_eq!(open.duration_ticks, 32);
        assert_eq!(close.duration_ticks, 32);
        assert_eq!(
            open.hooks,
            vec![PreparedDoorHook {
                offset_ticks: 2,
                ethereal: true
            }]
        );
        assert_eq!(
            close.hooks,
            vec![PreparedDoorHook {
                offset_ticks: 31,
                ethereal: false
            }]
        );
    }
    #[test]
    #[ignore = "requires approved retail DATs and accepted world pack"]
    fn approved_8602_doors_have_supported_links_and_hooks() {
        use bace_storage_codec::{PackLimits, load_manifest};
        use std::sync::Arc;
        let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
        let path = std::path::PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").unwrap());
        let manifest = load_manifest(&path, PackLimits::default()).unwrap();
        let hash = manifest.content_hash(PackLimits::default()).unwrap();
        let generation = Arc::new(
            manifest
                .open(path.parent().unwrap(), PackLimits::default())
                .unwrap(),
        );
        let mut assets = VerifiedRegionAssets::open(&RegionAssetManifest {
            portal: directory.join("client_portal.dat"),
            cell: directory.join("client_cell_1.dat"),
            portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
                .into(),
            cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
            store: None,
        })
        .unwrap();
        let region = assets
            .prepare(&RegionActivationRequest {
                token: 1,
                activation_epoch: 1,
                landblock: 0x8602,
                generation,
                creature_policy: None,
                treasure_assets: None,
                content_hash: hash,
                aetheria_drop_rate: 0.0,
            })
            .unwrap();
        let mut doors = 0;
        for instance in region
            .content
            .instances
            .iter()
            .filter(|i| !i.source.is_link_child && i.template.weenie_type == 19)
        {
            let motion_id = instance
                .template
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == 2)
                .unwrap()
                .value;
            let mut budget = 16 * 1024 * 1024;
            let table = assets.cached_motion(motion_id, &mut budget).unwrap();
            for (motion, ethereal) in [(0x4000000b, true), (0x4000000c, false)] {
                assets
                    .door_animation(&table, table.default_style, motion, ethereal, &mut budget)
                    .unwrap_or_else(|error| {
                        panic!(
                            "door {:#010x} motion {motion:#x} table {motion_id:#x}: {error}",
                            instance.source.guid
                        )
                    });
            }
            doors += 1;
        }
        assert!(doors > 0);
    }
}
