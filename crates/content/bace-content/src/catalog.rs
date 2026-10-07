use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{ContentError, ContentLimits, WeenieTemplate};

/// Immutable generation. Readers retain an Arc to this or a template across a
/// publication; a spawn resolves every index from the same generation.
#[derive(Clone, Debug, Default)]
pub struct CatalogSnapshot {
    revision: u64,
    by_id: BTreeMap<u32, Arc<WeenieTemplate>>,
    by_class: BTreeMap<String, u32>,
    by_type: BTreeMap<u32, BTreeSet<u32>>,
    by_display_name: BTreeMap<String, BTreeSet<u32>>,
}
impl CatalogSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn len(&self) -> usize {
        self.by_id.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
    pub fn get(&self, id: u32) -> Option<Arc<WeenieTemplate>> {
        self.by_id.get(&id).cloned()
    }
    pub fn by_class_name(&self, name: &str) -> Option<Arc<WeenieTemplate>> {
        self.by_class.get(name).and_then(|id| self.get(*id))
    }
    pub fn by_type(&self, kind: u32) -> Vec<u32> {
        self.by_type
            .get(&kind)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }
    pub fn by_display_name(&self, name: &str) -> Vec<u32> {
        self.by_display_name
            .get(name)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// Single-world-owner publication service. Hand immutable snapshots to readers;
/// the world thread installs a whole candidate at a tick boundary. Persistence
/// coordination and durable acceptance belong to the application/storage layer.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    active: Arc<CatalogSnapshot>,
}

#[derive(Debug)]
pub struct Publication {
    base_revision: u64,
    snapshot: Arc<CatalogSnapshot>,
}
impl Publication {
    pub fn snapshot(&self) -> Arc<CatalogSnapshot> {
        self.snapshot.clone()
    }
}
impl Catalog {
    pub fn snapshot(&self) -> Arc<CatalogSnapshot> {
        self.active.clone()
    }

    /// Builds every index off to the side; failures never touch the active state.
    /// Class-name uniqueness is checked after all replacements, permitting swaps.
    pub fn prepare(
        &self,
        revision: u64,
        templates: Vec<WeenieTemplate>,
        limits: ContentLimits,
    ) -> Result<Publication, ContentError> {
        if revision <= self.active.revision {
            return Err(ContentError::StaleRevision);
        }
        let mut candidate = CatalogSnapshot {
            revision,
            by_id: self.active.by_id.clone(),
            ..Default::default()
        };
        let mut batch = BTreeSet::new();
        for mut template in templates {
            template.validate(limits)?;
            if !batch.insert(template.weenie_id) {
                return Err(ContentError::DuplicateWeenie(template.weenie_id));
            }
            template.canonicalize();
            candidate
                .by_id
                .insert(template.weenie_id, Arc::new(template));
        }
        for (&id, template) in &candidate.by_id {
            if candidate
                .by_class
                .insert(template.class_name.clone(), id)
                .is_some()
            {
                return Err(ContentError::DuplicateClass(template.class_name.clone()));
            }
            candidate
                .by_type
                .entry(template.weenie_type)
                .or_default()
                .insert(id);
            if let Some(name) = template.properties.strings.iter().find(|p| p.id == 1) {
                candidate
                    .by_display_name
                    .entry(name.value.clone())
                    .or_default()
                    .insert(id);
            }
        }
        Ok(Publication {
            base_revision: self.active.revision,
            snapshot: Arc::new(candidate),
        })
    }

    pub fn publish(
        &mut self,
        candidate: Publication,
    ) -> Result<Arc<CatalogSnapshot>, ContentError> {
        if candidate.base_revision != self.active.revision {
            return Err(ContentError::StaleRevision);
        }
        self.active = candidate.snapshot;
        Ok(self.active.clone())
    }
}
