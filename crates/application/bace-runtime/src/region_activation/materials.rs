//! Cold verified DAT names used by pinned RecipeManager.GetMaterialName.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_material_names(&mut self) -> Result<BTreeMap<u32, String>, String> {
        let mut budget = 16 * 1024 * 1024;
        let mapper = bace_dat::DualDidMapper::decode_record(
            &read(
                &mut self.portal,
                bace_dat::DualDidMapper::MATERIAL_RECORD_ID,
                &mut budget,
            )?,
            bace_dat::DualDidMapper::MATERIAL_RECORD_ID,
            bace_dat::DatTableLimits::default(),
        )
        .map_err(|e| e.to_string())?;
        Ok(material_names(mapper))
    }
}

fn material_names(mapper: bace_dat::DualDidMapper) -> BTreeMap<u32, String> {
    mapper
        .client_names
        .into_iter()
        .map(|(id, name)| (id, name.replace('_', " ")))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_names_match_original_recipe_manager() {
        let hex =
            include_str!("../../../../assets/bace-dat/tests/fixtures/materials_mapper.hex").trim();
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let mapper = bace_dat::DualDidMapper::decode_record(
            &bytes,
            bace_dat::DualDidMapper::MATERIAL_RECORD_ID,
            Default::default(),
        )
        .unwrap();
        let names = material_names(mapper);
        for row in include_str!("../../../../assets/bace-dat/tests/fixtures/materials_mapper.csv")
            .lines()
            .filter(|l| l.starts_with("material,"))
        {
            let fields: Vec<_> = row.split(',').collect();
            assert_eq!(
                names.get(&fields[1].parse().unwrap()).map(String::as_str),
                Some(fields[2])
            );
        }
    }
}
