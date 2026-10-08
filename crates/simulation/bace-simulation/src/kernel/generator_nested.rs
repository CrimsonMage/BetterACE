//! Immutable template settings become a new machine only with accepted placement.
use super::*;
impl Kernel {
    pub fn register_prepared_generator_template(
        &mut self,
        revision: u64,
        template: u32,
        definition: Arc<GeneratorDefinition>,
    ) -> Result<(), GeneratorServiceError> {
        if revision == 0 || template == 0 || definition.identity.content_revision != revision {
            return Err(GeneratorServiceError::Invalid);
        }
        if self
            .generators
            .definitions
            .contains_key(&(revision, template))
        {
            return Err(GeneratorServiceError::Stale);
        }
        if self.generators.definitions.len() >= 4096 {
            return Err(GeneratorServiceError::Capacity);
        }
        self.generators
            .definitions
            .insert((revision, template), definition);
        Ok(())
    }
    pub(in crate::kernel) fn prepare_nested_generators(
        &mut self,
        intent: &GeneratorSpawnIntent,
        children: &[(EntityId, u32, GeneratorLocation)],
    ) -> Result<Vec<GeneratorMachine>, GeneratorServiceError> {
        let mut staged = Vec::new();
        for (ordinal, &(entity, template, location)) in children.iter().enumerate() {
            let Some(definition) = self
                .generators
                .definitions
                .get(&(intent.key.generator.content_revision, template))
            else {
                continue;
            };
            if self.generators.machines.contains_key(&entity)
                || staged
                    .iter()
                    .any(|m: &GeneratorMachine| m.definition().identity.entity == entity)
            {
                return Err(GeneratorServiceError::Stale);
            }
            if staged.len() >= self.generators.capacity - self.generators.machines.len() {
                return Err(GeneratorServiceError::Capacity);
            }
            let mut definition = (**definition).clone();
            let mut random = bace_spawning::generator_event_stream(
                self.generators
                    .root
                    .as_ref()
                    .ok_or(GeneratorServiceError::Missing)?,
                intent,
                u32::try_from(ordinal)
                    .map_err(|_| GeneratorServiceError::Capacity)?
                    .checked_add(0x10000)
                    .ok_or(GeneratorServiceError::Capacity)?,
            )
            .map_err(|_| GeneratorServiceError::Invalid)?;
            let mut random_identity = [0; 16];
            random_identity[..8].copy_from_slice(
                &random
                    .next_u64()
                    .map_err(|_| GeneratorServiceError::Invalid)?
                    .to_le_bytes(),
            );
            random_identity[8..].copy_from_slice(
                &random
                    .next_u64()
                    .map_err(|_| GeneratorServiceError::Invalid)?
                    .to_le_bytes(),
            );
            definition.identity = GeneratorIdentity {
                entity,
                incarnation: intent.key.occurrence,
                content_revision: intent.key.generator.content_revision,
                random_identity,
            };
            definition.location = location;
            definition.parent = Some(intent.key.generator.entity);
            let clock = self.generator_clock(definition.event.as_deref())?;
            staged.push(GeneratorMachine::new(
                Arc::new(definition),
                clock,
                GeneratorLimits::default(),
            )?);
        }
        let vendors = staged
            .iter()
            .filter(|m| m.definition().kind == GeneratorKind::Vendor)
            .count();
        if vendors > 4096 - self.generated_vendors.len()
            || staged.iter().any(|m| {
                m.definition().kind == GeneratorKind::Vendor
                    && self
                        .generated_vendors
                        .contains_key(&m.definition().identity.entity)
            })
        {
            return Err(GeneratorServiceError::Capacity);
        }
        Ok(staged)
    }
    pub(in crate::kernel) fn adopt_nested_generators(&mut self, machines: Vec<GeneratorMachine>) {
        for machine in machines {
            self.adopt_generator_vendor(&machine);
            self.generators
                .machines
                .insert(machine.definition().identity.entity, machine);
        }
    }
    pub(in crate::kernel) fn adopt_generator_vendor(&mut self, machine: &GeneratorMachine) {
        if machine.definition().kind == GeneratorKind::Vendor {
            self.generated_vendors.insert(
                machine.definition().identity.entity,
                crate::kernel::vendor_generators::GeneratedVendor {
                    stock: bace_economy::VendorStock::new(self.generators.capacity)
                        .expect("bounded positive generator capacity"),
                    items: Default::default(),
                    contents: Default::default(),
                    display_quantities: Default::default(),
                    lazy_loaded: false,
                    pending_lazy: None,
                    committed_lazy: None,
                    pending_buy: None,
                    marker_version: 0,
                    marker_stock_revision: 0,
                },
            );
        }
    }
}
