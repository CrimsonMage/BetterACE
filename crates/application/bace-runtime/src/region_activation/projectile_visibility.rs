//! Bounded cold rendering closure for the already selected spell projectile
//! templates. No instance identity or live accepted pose is manufactured here.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_projectile_visibility_templates(
        &mut self,
        sources: &[Arc<bace_content::WeenieV1>],
    ) -> Result<
        BTreeMap<u32, Arc<crate::visibility_assets::PreparedProjectileVisibilityTemplate>>,
        String,
    > {
        if sources.len() > 1024 {
            return Err("projectile visibility template capacity".into());
        }
        let source_refs: Vec<_> = sources.iter().map(AsRef::as_ref).collect();
        let appearance = self.prepare_entry_appearance(&source_refs)?;
        let chargen = bace_dat::CharGen::decode(
            &self
                .portal
                .read(bace_dat::CharGen::RECORD_ID)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let assets = appearance.borrowed(&chargen);
        let mut out = BTreeMap::new();
        for source in sources {
            let prepared =
                crate::visibility_assets::PreparedProjectileVisibilityTemplate::prepare_spell(
                    source.clone(),
                    &assets,
                )?;
            if out
                .insert(prepared.template(), Arc::new(prepared))
                .is_some()
            {
                return Err("duplicate projectile visibility template".into());
            }
        }
        Ok(out)
    }
}
