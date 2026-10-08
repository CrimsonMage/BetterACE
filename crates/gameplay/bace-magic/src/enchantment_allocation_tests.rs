use super::*;
#[test]
fn empty_registries_retain_bounds_without_preallocating_the_worst_case() {
    let registries = (0..4096)
        .map(|_| EnchantmentRegistry::new(512).unwrap())
        .collect::<Vec<_>>();
    assert!(
        registries
            .iter()
            .all(|r| r.capacity() == 512 && r.entries.is_empty())
    );
    assert_eq!(
        registries
            .iter()
            .map(|r| r.entries.capacity())
            .sum::<usize>(),
        0
    );
    let avoided = 4096usize * 512 * std::mem::size_of::<EnchantmentEntry>();
    assert!(avoided >= 128 * 1024 * 1024);
    assert!(EnchantmentRegistry::new(0).is_err());
    assert!(EnchantmentRegistry::new(4097).is_err());
}
