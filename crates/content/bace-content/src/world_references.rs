//! Deterministic cross-record diagnostics. Missing source references are retained,
//! never removed or substituted during conversion. Runtime admission must resolve
//! the required reference/geometry for each activation or reject that activation.
use crate::{WeenieTemplate, WorldRecordV1};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldReferenceIssue {
    pub owner_kind: &'static str,
    pub owner_id: u64,
    pub field: &'static str,
    pub target: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldReferenceReport {
    pub checked: u64,
    pub missing: u64,
    pub missing_by_field: BTreeMap<&'static str, u64>,
    /// At most 1,024 deterministic examples; total counts are never truncated.
    pub examples: Vec<WorldReferenceIssue>,
}
impl WorldReferenceReport {
    fn check(
        &mut self,
        exists: bool,
        owner_kind: &'static str,
        owner_id: u64,
        field: &'static str,
        target: u32,
    ) {
        self.checked += 1;
        if !exists {
            self.missing += 1;
            *self.missing_by_field.entry(field).or_default() += 1;
            if self.examples.len() < 1024 {
                self.examples.push(WorldReferenceIssue {
                    owner_kind,
                    owner_id,
                    field,
                    target,
                });
            }
        }
    }
}
/// Audit authored references without assuming every positive generator value is
/// an ordinary weenie: ACE also interprets treasure-generator fields by flags.
/// Zero optional IDs are sentinel values, excluded from this presence audit.
pub fn inspect_world_references(
    weenies: &[WeenieTemplate],
    records: &[WorldRecordV1],
) -> WorldReferenceReport {
    let ids: BTreeSet<_> = weenies.iter().map(|w| w.weenie_id).collect();
    let instances: BTreeSet<_> = records
        .iter()
        .filter_map(|r| match r {
            WorldRecordV1::LandblockInstance(r) => Some(r.guid),
            _ => None,
        })
        .collect();
    let mut report = WorldReferenceReport::default();
    let mut ordered: Vec<_> = weenies.iter().collect();
    ordered.sort_unstable_by_key(|w| w.weenie_id);
    for w in ordered {
        for (field, targets) in [
            (
                "create_list.weenie_class_id",
                w.properties
                    .create_list
                    .iter()
                    .map(|r| r.weenie_class_id)
                    .collect::<Vec<_>>(),
            ),
            (
                "generator.weenie_class_id",
                w.properties
                    .generators
                    .iter()
                    .map(|r| r.weenie_class_id)
                    .collect(),
            ),
            (
                "emote.weenie_class_id",
                w.properties
                    .emotes
                    .iter()
                    .filter_map(|r| r.weenie_class_id)
                    .collect(),
            ),
            (
                "emote_action.weenie_class_id",
                w.properties
                    .emotes
                    .iter()
                    .flat_map(|r| &r.actions)
                    .filter_map(|r| r.weenie_class_id)
                    .collect(),
            ),
        ] {
            for target in targets {
                if target != 0 {
                    report.check(
                        ids.contains(&target),
                        "weenie",
                        u64::from(w.weenie_id),
                        field,
                        target,
                    );
                }
            }
        }
    }
    let mut ordered: Vec<_> = records.iter().collect();
    ordered.sort_unstable_by_key(|r| (r.namespace(), r.id()));
    for r in ordered {
        let target = match r {
            WorldRecordV1::LandblockInstance(r) => {
                Some(("landblock_instance.weenie_class_id", r.weenie_class_id))
            }
            WorldRecordV1::Encounter(r) => Some(("encounter.weenie_class_id", r.weenie_class_id)),
            WorldRecordV1::TreasureWielded(r) => {
                Some(("treasure_wielded.weenie_class_id", r.weenie_class_id))
            }
            WorldRecordV1::PointsOfInterest(r) => {
                Some(("points_of_interest.weenie_class_id", r.weenie_class_id))
            }
            _ => None,
        };
        if let Some((field, target)) = target
            && target != 0
        {
            report.check(ids.contains(&target), r.table_name(), r.id(), field, target);
        }
        if let WorldRecordV1::LandblockInstanceLink(link) = r {
            report.check(
                instances.contains(&link.parent_guid),
                r.table_name(),
                r.id(),
                "instance_link.parent_guid",
                link.parent_guid,
            );
            report.check(
                instances.contains(&link.child_guid),
                r.table_name(),
                r.id(),
                "instance_link.child_guid",
                link.child_guid,
            );
        }
    }
    report
}
