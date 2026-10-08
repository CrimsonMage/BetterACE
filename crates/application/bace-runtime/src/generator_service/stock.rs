//! Vendor stock retains constructed descendants under the vendor's sole owner.
use super::*;
use bace_inventory::ItemPlace;
use bace_loot::PreparedContainerItem;
use bace_simulation::{GeneratorItemAdmission, PreparedVendorTree};
use std::collections::BTreeSet;
pub(super) fn bind(
    request: &GeneratorHostRequest,
    raw: &[PreparedContainerItem],
) -> Result<materialization::Ready, String> {
    let mut trees: Vec<PreparedVendorTree> = Vec::new();
    let mut tree_for: Vec<usize> = Vec::with_capacity(raw.len());
    for (index, item) in raw.iter().enumerate() {
        let id = request.entities[index];
        let mut source = item.source.clone();
        let generator = item
            .generator_parent_index
            .map_or(request.intent.key.generator.entity, |n| request.entities[n]);
        if let Some(p) = source
            .properties
            .instance_ids
            .iter_mut()
            .find(|p| p.id == 6)
        {
            p.value = generator.0;
        } else {
            source.properties.instance_ids.push(bace_content::Property {
                id: 6,
                value: generator.0,
            });
            source.properties.instance_ids.sort_by_key(|p| p.id);
        }
        if let Some(parent) = item.parent_index {
            if parent >= index {
                return Err("invalid vendor child ordering".into());
            }
            let tree_index = tree_for[parent];
            tree_for.push(tree_index);
            let (projected, container) = crate::generator_items::prepare_inventory_item(
                &source,
                id,
                1,
                ItemPlace::Contained {
                    container: request.entities[parent],
                    slot: item.inventory_slot,
                    equipped: 0,
                },
            )?;
            let tree = &mut trees[tree_index];
            tree.items.push(projected);
            tree.containers.extend(container);
            tree.templates.insert(id, Arc::new(source));
        } else {
            let (_, container) =
                crate::generator_items::prepare_inventory_item(&source, id, 1, ItemPlace::World)?;
            tree_for.push(trees.len());
            trees.push(PreparedVendorTree {
                root: id,
                template: Arc::new(source),
                items: vec![],
                containers: container.into_iter().collect(),
                templates: BTreeMap::new(),
            });
        }
    }
    Ok(materialization::Ready {
        action: GeneratorAction::AdmitStockTrees {
            key: request.intent.key,
            trees,
        },
        sources: vec![],
    })
}
pub(super) fn valid_receipt(action: &GeneratorAction, receipt: &GeneratorItemAdmission) -> bool {
    let GeneratorAction::AdmitStockTrees { key, trees } = action else {
        return false;
    };
    if *key != receipt.key
        || !receipt.failed_roots.is_empty()
        || receipt.roots.len() != trees.len()
        || receipt.roots.iter().any(|id| id.0 == 0)
    {
        return false;
    }
    let accepted: BTreeSet<_> = receipt.entities.iter().copied().collect();
    if accepted.len() != receipt.entities.len() {
        return false;
    }
    let mut expected = BTreeSet::new();
    for (tree, retained) in trees.iter().zip(&receipt.roots) {
        if accepted.contains(&tree.root) {
            if *retained != tree.root {
                return false;
            }
            expected.insert(tree.root);
            expected.extend(tree.items.iter().map(|i| i.id));
        } else if *retained == tree.root {
            return false;
        }
    }
    accepted == expected
}
