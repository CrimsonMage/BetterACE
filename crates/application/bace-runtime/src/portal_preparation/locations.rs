//! Pinned Player_Location static destinations, resolved from the accepted class
//! index on cold startup capacity. No full-world template scan or geometry I/O.
use bace_content::TemplateClassIndexV1;
use bace_interactions::PortalPosition;
use bace_simulation::PreparedRecallLocations;
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};

/// ACE snapshots these eleven class destinations at startup. A missing class or
/// absent Destination uses its source fallback; a corrupt accepted record fails
/// startup. This does not admit destination geometry or override disabled zones.
#[expect(
    clippy::approx_constant,
    reason = "ACE authored fallback float bits are source data, not mathematical constants"
)]
pub fn prepare_recall_locations(
    generation: &PackGeneration,
    classes: &TemplateClassIndexV1,
) -> Result<PreparedRecallLocations, String> {
    classes.validate()?;
    let mut budget = 16 * 1024 * 1024usize;
    let mut resolve = |name: &str, fallback: PortalPosition| -> Result<PortalPosition, String> {
        let Some(identity) = classes
            .entries
            .iter()
            .find(|entry| entry.class_name == name)
        else {
            return Ok(fallback);
        };
        let PackLookup::Record(record) = generation
            .lookup(PackKey {
                namespace: 1,
                id: u64::from(identity.template),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err(format!(
                "recall class {name} points to a missing accepted template"
            ));
        };
        budget = budget
            .checked_sub(record.bytes().len())
            .ok_or("recall template byte capacity")?;
        if record.schema() != 1 {
            return Err("recall template schema mismatch".into());
        }
        let source = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
        if source.weenie_id != identity.template || source.class_name != name {
            return Err("recall class/template identity mismatch".into());
        }
        let destination = source
            .properties
            .positions
            .iter()
            .find(|p| p.id == 2)
            .map_or(fallback, |p| super::position(&p.value));
        destination
            .validate()
            .map_err(|e| format!("recall destination {name}: {e:?}"))?;
        Ok(destination)
    };
    let marketplace = resolve(
        "portalmarketplace",
        PortalPosition {
            cell: 0x016c01bc,
            origin: [49.206, -31.935, 0.005],
            rotation: [0.707107, 0., 0., -0.707107],
        },
    )?;
    let mut pk_arena = arena_fallbacks(0x0066);
    let mut pkl_arena = arena_fallbacks(0x0067);
    for (i, (pk, pkl)) in pk_arena.iter_mut().zip(&mut pkl_arena).enumerate() {
        *pk = resolve(&format!("portalpkarenanew{}", i + 1), *pk)?;
        *pkl = resolve(&format!("portalpklarenanew{}", i + 1), *pkl)?;
    }
    Ok(PreparedRecallLocations {
        marketplace,
        pk_arena,
        pkl_arena,
    })
}
fn arena_fallbacks(landblock: u32) -> [PortalPosition; 5] {
    [
        (0x0117, [30., -50., 0.005], [1., 0., 0., 0.]),
        (0x0106, [10., 0., 0.005], [0.321023, 0., 0., -0.947071]),
        (0x0103, [30., -30., 0.005], [0.714424, 0., 0., -0.699713]),
        (0x011e, [50., 0., 0.005], [-0.276474, 0., 0., -0.961021]),
        (0x0127, [60., -30., 0.005], [0.731689, 0., 0., 0.681639]),
    ]
    .map(|(cell, origin, rotation)| PortalPosition {
        cell: (landblock << 16) | cell,
        origin,
        rotation,
    })
}
