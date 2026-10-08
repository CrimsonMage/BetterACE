use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub fn build(
    tables: &[PathBuf],
    profiles: &[PathBuf],
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.exists() {
        return Err("loot-build requires a new output directory".into());
    }
    if tables.len() + profiles.len() > 4096 {
        return Err("too many profile inputs".into());
    }
    let graphs = tables
        .iter()
        .map(|p| read(p).and_then(|s| Ok(bace_content_tools::parse_loot_graph(&s)?)))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let rares = profiles
        .iter()
        .map(|p| read(p).and_then(|s| Ok(bace_content_tools::parse_rare_profile(&s)?)))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let result = bace_content_tools::build_loot_pack(&graphs, &rares, output)?;
    println!(
        "Compiled {} profiles into one .bace supplement: {}",
        result.records,
        result.file.display()
    );
    println!(
        "Manifest: {}. Not activated; publication must validate template references and the complete catalog.",
        result.manifest.display()
    );
    for rare in rares.iter().filter(|r| !r.enabled) {
        println!(
            "Rare profile {} is disabled/unconfigured; no retail probability was assumed.",
            rare.id
        );
    }
    Ok(())
}
pub fn sample(input: &Path, events: u32) -> Result<(), Box<dyn std::error::Error>> {
    if events == 0 || events > 100000 {
        return Err("sample events must be 1..=100000".into());
    }
    let graph = bace_loot::LootGraph::prepare(bace_content_tools::parse_loot_graph(&read(input)?)?)
        .map_err(|e| format!("{e:?}"))?;
    let root = bace_random::RandomRoot::new([0x42; 32], 1).map_err(|e| format!("{e:?}"))?;
    let mut scratch = bace_loot::LootScratch::default();
    let mut output = Vec::with_capacity(256);
    let mut counts = BTreeMap::<u32, (u64, u64, u64)>::new();
    let mut empty = 0u64;
    for event in 1..=events {
        output.clear();
        graph
            .generate(
                &root,
                u128::from(event).to_le_bytes(),
                &mut output,
                &mut scratch,
            )
            .map_err(|e| format!("{e:?}"))?;
        let mut seen = BTreeSet::new();
        if output.is_empty() {
            empty += 1;
        }
        for item in &output {
            let row = counts.entry(item.template).or_default();
            row.1 += 1;
            row.2 += u64::from(item.stack);
            if seen.insert(item.template) {
                row.0 += 1;
            }
        }
    }
    println!(
        "Synthetic deterministic sample: {events} events, {empty} empty. No player RNG or rare stream used."
    );
    println!("template,events_with_item,stacks,units");
    for (template, (hits, stacks, units)) in counts {
        println!("{template},{hits},{stacks},{units}");
    }
    Ok(())
}
fn read(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = String::new();
    std::fs::File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("loot authoring input exceeds 16MiB".into());
    }
    Ok(bytes)
}

/// Queue native changes into the same durable journal as SQL authoring. No head
/// changes here: the native mapped publication worker validates references first.
pub async fn publish(
    tables: &[PathBuf],
    profiles: &[PathBuf],
    store: &bace_db_postgres::PgStore,
) -> Result<i64, Box<dyn std::error::Error>> {
    if tables.len() + profiles.len() == 0 || tables.len() + profiles.len() > 4096 {
        return Err("native profile input count".into());
    }
    let mut candidates = Vec::with_capacity(tables.len() + profiles.len());
    let mut total = 0usize;
    let mut seen = BTreeSet::new();
    for (paths, namespace) in [(tables, 46), (profiles, 47)] {
        for path in paths {
            let source = read(path)?;
            let (id, bytes) = if namespace == 46 {
                let value = bace_content_tools::parse_loot_graph(&source)?;
                (value.id, bace_content_tools::compile_loot_graph(&value)?)
            } else {
                let value = bace_content_tools::parse_rare_profile(&source)?;
                (value.id, bace_content_tools::compile_rare_profile(&value)?)
            };
            total = total
                .checked_add(bytes.len())
                .ok_or("native profile byte overflow")?;
            if total > 16 * 1024 * 1024 || !seen.insert((namespace, id)) {
                return Err("native profile byte limit or duplicate identity".into());
            }
            candidates.push(bace_persistence::NativeContentCandidate {
                namespace,
                id,
                schema: 1,
                bytes,
            });
        }
    }
    Ok(store.insert_native_candidates(&candidates).await?)
}
