//! A summoned portal enters the accepted visibility owner only after its
//! immutable model has been prepared off the runtime event pump.
use super::*;
use crate::visibility_assets::{PreparedVisibilityObject, VisibilitySource};
use bace_storage_codec::{PackKey, PackLookup};

// ACE WorldObject_Magic.SummonPortal constructs "portalgateway" (WCID 1955),
// then copies the linked portal's destination and access properties onto it.
// The original portal remains in the accepted ticket/anchor, not the visual base.
const PORTAL_GATEWAY_TEMPLATE: u32 = 1955;

fn load_gateway_visual(
    generation: &bace_storage_codec::PackGeneration,
    template: u32,
) -> Result<bace_content::WeenieV1, String> {
    if template != PORTAL_GATEWAY_TEMPLATE {
        return Err("summoned portal gateway template mismatch".into());
    }
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(template),
        })
        .map_err(|error| error.to_string())?
    else {
        return Err("summoned portal gateway visual missing".into());
    };
    let source: bace_content::WeenieV1 =
        bace_content_tools::decode(record.bytes()).map_err(|error| error.to_string())?;
    if source.weenie_id != template || source.class_name != "portalgateway" {
        return Err("summoned portal gateway source identity mismatch".into());
    }
    Ok(source)
}

impl GameRuntime {
    pub(super) fn poll_portal_summons(&mut self) -> Result<(), String> {
        if let Some((operation, entity, template, result)) = ready(&mut self.portals.summon_job) {
            let prepared =
                result.map_err(|error| format!("summoned portal blueprint retained: {error}"))?;
            self.portals.summon_ready = Some((operation, entity, template, prepared));
        }
        let Some(delivery) = self.portals.deliveries.front() else {
            return Ok(());
        };
        let PortalDeliveryWork::Event(PortalServiceEvent::Summoned {
            operation,
            entity,
            template,
        }) = &delivery.work
        else {
            return Ok(());
        };
        let (operation, entity, template) = (*operation, *entity, *template);
        // The cold job may finish after another completion/control channel
        // advances. Keep the exact committed ticket until visibility owns the
        // same summon identity, even on the ready-result fast path.
        let ticket = self
            .portals
            .tickets
            .get(&operation)
            .ok_or("summoned portal ticket missing")?;
        let bace_simulation::PortalServiceEffect::Summon {
            entity: accepted_entity,
            template: accepted_template,
            original_template,
            ..
        } = &ticket.effect
        else {
            return Err("summoned portal ticket effect mismatch".into());
        };
        if *accepted_entity != entity || *accepted_template != template {
            return Err("summoned portal effect identity mismatch".into());
        }
        if *original_template == 0 {
            return Err("summoned portal original link missing".into());
        }
        if let Some((ready_operation, ready_entity, ready_template, prepared)) =
            self.portals.summon_ready.as_ref()
        {
            if (*ready_operation, *ready_entity, *ready_template) != (operation, entity, template) {
                return Err("summoned portal blueprint identity changed".into());
            }
            let tick =
                u64::try_from(self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000)
                    .map_err(|_| "summoned portal admission tick overflow")?;
            prepared
                .register(&mut self.visibility.service, tick)
                .map_err(|error| format!("summoned portal visibility retained: {error:?}"))?;
            self.acknowledge_summoned_portal(entity, template)?;
            self.portals.summon_ready = None;
            return Ok(());
        }
        if self.portals.summon_job.is_some() {
            return Ok(());
        }
        let generation = self.bootstrap.pack.generation.clone();
        let manifest = self.bootstrap.assets.clone();
        self.portals.summon_job = Some(Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || {
                let source = load_gateway_visual(&generation, template)?;
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                let mut prepared = assets.prepare_visibility_sources(vec![VisibilitySource {
                    entity,
                    incarnation: operation,
                    revision: generation.revision().max(1),
                    source: &source,
                    equipment: vec![],
                    missile_combat: false,
                }])?;
                let prepared: PreparedVisibilityObject =
                    prepared.pop().ok_or("summoned portal blueprint missing")?;
                Ok(prepared)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|value| value);
            (operation, entity, template, result)
        }));
        Ok(())
    }
}

#[cfg(test)]
#[path = "summons/tests.rs"]
mod tests;
