use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
    RegistryError,
};

fn entry(spell: u32, layer: u16, duration: f64) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster: 100,
        school: MagicSchool::Creature,
        spec: EnchantmentSpec {
            category: 23,
            power: 100,
            duration,
            layer,
            stat_type: 0x1000,
            stat_key: 7,
            value: 12.5,
            beneficial: true,
            set_id: Some(42),
        },
        start_time: -15.0,
        is_set_spell: true,
        is_level8_aura: false,
        metadata: EnchantmentMetadata {
            enchantment_category: 12,
            has_spell_set_id: true,
            degrade_modifier: 0.25,
            degrade_limit: 0.5,
            last_time_degraded: -10.0,
            spell_set_id: 42,
        },
    }
}

#[test]
fn restoration_preserves_layers_elapsed_time_and_all_metadata() {
    let mut other = entry(123, 9, 120.0);
    other.caster = 200;
    let entries = vec![entry(123, 3, 60.0), other, entry(666, 1, -1.0)];
    let registry = EnchantmentRegistry::restore(32, 81, entries.clone()).unwrap();
    assert_eq!(registry.revision(), 81);
    assert_eq!(registry.entries(), entries);
    assert_eq!(registry.into_entries(), entries);
}

#[test]
fn invalid_restore_returns_all_input_without_partial_admission() {
    let entries = vec![entry(123, 1, 60.0), entry(123, 1, 120.0)];
    let (error, recovered) = EnchantmentRegistry::restore(32, 9, entries.clone()).unwrap_err();
    assert_eq!(error, RegistryError::InvalidEntry);
    assert_eq!(entries, recovered);
    let entries = vec![entry(123, 1, 60.0), entry(124, 1, 120.0)];
    let (error, recovered) = EnchantmentRegistry::restore(1, 9, entries.clone()).unwrap_err();
    assert_eq!(error, RegistryError::Capacity);
    assert_eq!(entries, recovered);
}

#[test]
fn nonfinite_stored_metadata_and_invalid_duration_are_rejected() {
    for field in 0..5 {
        let mut row = entry(123, 1, 60.0);
        match field {
            0 => row.metadata.degrade_modifier = f32::INFINITY,
            1 => row.metadata.degrade_limit = f32::NAN,
            2 => row.metadata.last_time_degraded = f64::INFINITY,
            3 => row.start_time = f64::NAN,
            _ => row.spec.duration = -2.0,
        }
        assert_eq!(
            EnchantmentRegistry::restore(1, 0, vec![row]).unwrap_err().0,
            RegistryError::InvalidEntry
        );
    }
}

#[test]
fn active_heartbeat_boundary_matches_pinned_ace_relative_clock() {
    // ACE 47edade3, PropertiesEnchantmentRegistryExtensions.cs:235-264:
    // start -= interval, expire finite rows at start <= -duration.
    let rows = vec![
        entry(123, 3, 20.0),
        entry(124, 8, 20.001),
        entry(666, 1, -1.0),
    ];
    let mut registry = EnchantmentRegistry::restore(8, 20, rows).unwrap();
    let mut removed = Vec::with_capacity(8);
    registry.heartbeat(5.0, &mut removed).unwrap();
    assert_eq!(removed, [(123, 3)]);
    assert_eq!(
        registry
            .entries()
            .iter()
            .map(|r| (r.spell, r.start_time, r.spec.duration))
            .collect::<Vec<_>>(),
        [(124, -20.0, 20.001), (666, -20.0, -1.0)]
    );
    assert_eq!(registry.revision(), 21);
}

#[test]
fn saturated_removal_output_preserves_complete_registry_and_revision() {
    let original = vec![entry(123, 3, 20.0)];
    let mut registry = EnchantmentRegistry::restore(8, 20, original.clone()).unwrap();
    let mut removed = Vec::new();
    assert_eq!(
        registry.heartbeat(5.0, &mut removed),
        Err(RegistryError::OutputCapacity)
    );
    assert_eq!(registry.entries(), original);
    assert_eq!(registry.revision(), 20);
    removed.reserve(8);
    registry.heartbeat(5.0, &mut removed).unwrap();
    assert_eq!(removed, [(123, 3)]);
    assert!(registry.entries().is_empty());
}

#[test]
fn save_reload_does_not_advance_offline_time_or_refresh_spell() {
    let mut registry = EnchantmentRegistry::restore(8, 20, vec![entry(123, 3, 60.0)]).unwrap();
    let mut output = Vec::with_capacity(8);
    registry.heartbeat(5.0, &mut output).unwrap();
    let revision = registry.revision();
    let mut restored = EnchantmentRegistry::restore(8, revision, registry.into_entries()).unwrap();
    assert_eq!(restored.entries()[0].start_time, -20.0);
    restored.heartbeat(40.0, &mut output).unwrap();
    assert_eq!(output, [(123, 3)]);
}

#[test]
fn stale_add_proposal_does_not_overwrite_a_saved_registry_mutation() {
    let mut registry = EnchantmentRegistry::restore(8, 20, vec![entry(123, 3, 60.0)]).unwrap();
    let proposal = registry
        .propose_add(entry(124, 0, 60.0), 0.0, false)
        .unwrap();
    let mut output = Vec::with_capacity(8);
    registry.heartbeat(5.0, &mut output).unwrap();
    assert_eq!(
        registry.adopt(proposal),
        Err(RegistryError::RevisionExhausted)
    );
    assert_eq!(registry.entries()[0].start_time, -20.0);
}

#[test]
fn login_vitae_cleanup_is_an_explicit_revisioned_domain_mutation() {
    let mut vitae = entry(666, 1, -1.0);
    vitae.spec.value = 0.99995;
    let mut registry = EnchantmentRegistry::restore(8, 20, vec![vitae]).unwrap();
    assert!(registry.normalize_vitae().unwrap());
    assert_eq!(registry.revision(), 21);
    assert!(registry.entries().is_empty());
    assert!(!registry.normalize_vitae().unwrap());
    assert_eq!(registry.revision(), 21);
    let mut penalized = entry(666, 1, -1.0);
    penalized.spec.value = 0.95;
    let mut registry = EnchantmentRegistry::restore(8, 20, vec![penalized]).unwrap();
    assert!(!registry.normalize_vitae().unwrap());
    assert_eq!(registry.entries().len(), 1);
}
