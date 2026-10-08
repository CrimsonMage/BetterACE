use bace_dat::{
    DatArchive, DatPalette, DatPaletteSet, DatSurface, DatTexture, GraphicsObject, ModelSetup,
};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Default)]
pub(crate) struct PreviewRequest {
    pub path: PathBuf,
    pub setup: u32,
    pub clothing: u32,
    pub palette: u32,
    pub template: u32,
    pub shade: f64,
    pub appearance: Option<bace_content::WeenieV1>,
    pub clothing_patch: Option<bace_content::ClothingPatchV1>,
    pub animation_id: u32,
    pub animation_frame: usize,
    pub animation_patch: Option<bace_content::AnimationSwapPatchV1>,
}
pub(crate) struct PreviewScene {
    pub clothing: Option<bace_content::ClothingPatchV1>,
    pub tables: Option<(bace_dat::SkillTable, bace_dat::VitalTable)>,
    pub triangles: Vec<Triangle>,
    pub textures: Vec<Texture>,
    pub bounds: ([f32; 3], [f32; 3]),
    pub report: Vec<String>,
    pub colors: Vec<u32>,
    pub ids: Vec<u32>,
}
pub(crate) struct Texture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 4]>,
}
#[derive(Clone)]
pub(crate) struct Triangle {
    pub points: [[f32; 3]; 3],
    pub uvs: [[f32; 2]; 3],
    pub texture: usize,
    pub shade: f32,
    pub cull: bool,
}

pub(crate) fn load(request: PreviewRequest) -> Result<PreviewScene, String> {
    let mut archive = DatArchive::open(&request.path).map_err(|e| e.to_string())?;
    if archive.header().dataset != 1 {
        return Err("Choose a Portal DAT (client_portal.dat).".into());
    }
    let ids = archive
        .records()
        .keys()
        .copied()
        .filter(|id| matches!(id >> 24, 1 | 2 | 4 | 15 | 16))
        .take(200_000)
        .collect();
    let mut scene = PreviewScene {
        clothing: None,
        tables: None,
        triangles: Vec::new(),
        textures: Vec::new(),
        bounds: ([f32::MAX; 3], [f32::MIN; 3]),
        report: Vec::new(),
        colors: Vec::new(),
        ids,
    };
    match (
        read(&mut archive, 0x0e000004)
            .and_then(|bytes| bace_dat::SkillTable::decode(&bytes).map_err(|e| e.to_string())),
        read(&mut archive, 0x0e000003)
            .and_then(|bytes| bace_dat::VitalTable::decode(&bytes).map_err(|e| e.to_string())),
    ) {
        (Ok(skills), Ok(vitals)) => scene.tables = Some((skills, vitals)),
        _ => scene
            .report
            .push("Skill/vital tables could not be decoded; calculators unavailable.".into()),
    }
    let clothing = crate::preview_clothing::resolve(
        &mut archive,
        request.clothing,
        request.clothing_patch.as_ref(),
    )?;
    let mut patches: Vec<(u32, u32, u32)> = Vec::new();
    if let Some(clothing) = &clothing {
        scene.report.push(format!(
            "ClothingBase {:08X} · {} setups · {} palette templates",
            clothing.id,
            clothing.setups.len(),
            clothing.palettes.len()
        ));
        if request
            .clothing_patch
            .as_ref()
            .is_some_and(|p| p.id == clothing.id)
        {
            scene
                .report
                .push("Native override merged with original DAT by setup/template key".into());
        }
        for setup in &clothing.setups {
            scene.report.push(format!(
                "Setup {:08X}: {} model replacements",
                setup.id,
                setup.parts.len()
            ));
            if setup.id == request.setup {
                for part in &setup.parts {
                    scene.report.push(format!(
                        "  Part {} → {:08X} · {} texture changes",
                        part.index,
                        part.model,
                        part.textures.len()
                    ));
                    for t in &part.textures {
                        scene
                            .report
                            .push(format!("    {:08X} → {:08X}", t.old, t.new));
                    }
                }
            }
        }
        for template in &clothing.palettes {
            scene.report.push(format!(
                "Template {} · icon {:08X} · {} palette effects",
                template.template,
                template.icon,
                template.effects.len()
            ));
            for effect in &template.effects {
                scene
                    .report
                    .push(format!("  {:?} · {:?}", effect.source, effect.ranges));
            }
        }
        if request.setup != 0 && !clothing.setups.iter().any(|s| s.id == request.setup) {
            return Err(format!(
                "ClothingBase {:08X} has no entry for Setup {:08X}. Inspect DIDs to choose a supported setup.",
                clothing.id, request.setup
            ));
        }
        if let Some(template) = clothing
            .palettes
            .iter()
            .find(|p| p.template == request.template)
        {
            for effect in &template.effects {
                let id = match effect.source {
                    bace_content::PaletteSource::Palette(id) => id,
                    bace_content::PaletteSource::PaletteSet(id) => {
                        bace_dat::DatPaletteSet::decode(&read(&mut archive, id)?)
                            .map_err(|e| e.to_string())?
                            .at_shade(request.shade)
                            .ok_or("Palette shade must be between 0 and 1")?
                    }
                };
                for range in &effect.ranges {
                    patches.push((
                        id,
                        range.offset,
                        if range.colors == 0 {
                            2048
                        } else {
                            range.colors
                        },
                    ));
                }
            }
        } else if request.setup != 0 && !clothing.palettes.is_empty() {
            return Err(format!(
                "Clothing table has no palette template {}. Inspect DIDs to choose one.",
                request.template
            ));
        }
    }
    if let Some(table) = &clothing
        && let Err(index) = scene.ids.binary_search(&table.id)
    {
        scene.ids.insert(index, table.id);
    }
    scene.clothing = clothing.clone();
    if request.palette != 0 {
        if request.palette >> 24 == 15 {
            let set = DatPaletteSet::decode(&read(&mut archive, request.palette)?)
                .map_err(|e| e.to_string())?;
            scene.report.push(format!(
                "PaletteSet {:08X}: {} palettes",
                set.id,
                set.palettes.len()
            ));
            for id in &set.palettes {
                scene.report.push(format!("  {id:08X}"));
            }
            let id = set
                .at_shade(request.shade)
                .ok_or("Palette set is empty or shade is outside 0..1")?;
            scene.colors = DatPalette::decode(&read(&mut archive, id)?)
                .map_err(|e| e.to_string())?
                .colors;
        } else {
            scene.colors = DatPalette::decode(&read(&mut archive, request.palette)?)
                .map_err(|e| e.to_string())?
                .colors;
        }
    }
    let mut models = Vec::new();
    if request.setup >> 24 == 1 {
        models.push((
            request.setup,
            [1.0; 3],
            bace_dat::ModelFrame {
                origin: [0.0; 3],
                rotation: [1.0, 0.0, 0.0, 0.0],
            },
        ));
    } else if request.setup != 0 {
        let setup =
            ModelSetup::decode(&read(&mut archive, request.setup)?).map_err(|e| e.to_string())?;
        let frames = setup
            .placements
            .get(&0)
            .or_else(|| setup.placements.get(&1))
            .or_else(|| setup.placements.values().next())
            .ok_or("Setup has no placement frames")?;
        scene.report.push(format!(
            "Setup {:08X} · {} parts · default motion {:08X}",
            setup.id,
            setup.parts.len(),
            setup.default_motion
        ));
        for (i, &model) in setup.parts.iter().enumerate() {
            models.push((model, setup.scales[i], frames[i].clone()));
        }
    }
    let mut replacements: BTreeMap<(usize, u32), u32> = BTreeMap::new();
    if let Some(clothing) = &clothing
        && let Some(setup) = clothing.setups.iter().find(|s| s.id == request.setup)
    {
        for part in &setup.parts {
            let model = models
                .get_mut(part.index as usize)
                .ok_or("Clothing part index exceeds setup")?;
            model.0 = part.model;
            for t in &part.textures {
                replacements.insert((part.index as usize, t.old), t.new);
            }
        }
    }
    if let Some(weenie) = &request.appearance {
        for part in &weenie.properties.animation_parts {
            models
                .get_mut(usize::from(part.index))
                .ok_or("Animation part index exceeds setup")?
                .0 = part.animation_id;
        }
        for texture in &weenie.properties.texture_maps {
            replacements.insert(
                (usize::from(texture.part_index), texture.old_texture),
                texture.new_texture,
            );
        }
        for palette in &weenie.properties.palettes {
            patches.push((
                // ACE can retain a full DID or the compact low word in ObjDesc.
                if palette.sub_palette_id <= u16::MAX.into() {
                    0x04000000 | palette.sub_palette_id
                } else {
                    palette.sub_palette_id
                },
                u32::from(palette.offset) * 8,
                if palette.length == 0 {
                    2048
                } else {
                    u32::from(palette.length) * 8
                },
            ));
        }
    }
    if request.animation_id != 0 {
        let animation = bace_dat::Animation::decode(&read(&mut archive, request.animation_id)?)
            .map_err(|e| e.to_string())?;
        let animation = if let Some(patch) = &request.animation_patch {
            bace_asset_overlay::resolve_animation_swaps(&animation, patch)?
        } else {
            animation
        };
        let frame = animation
            .frames
            .get(request.animation_frame)
            .ok_or("Animation frame is missing")?;
        if frame.parts.len() != models.len() {
            return Err("Animation part count does not match the selected Setup".into());
        }
        for (model, pose) in models.iter_mut().zip(&frame.parts) {
            model.2 = pose.clone();
        }
        for (frame_index, frame) in animation
            .frames
            .iter()
            .enumerate()
            .take(request.animation_frame + 1)
        {
            for hook in &frame.hooks {
                if hook.direction < 0 {
                    continue;
                }
                if let bace_dat::AnimationHookPayload::ReplaceObject {
                    part_index,
                    object_id,
                    ..
                } = hook.payload
                {
                    let part = models
                        .get_mut(usize::from(part_index))
                        .ok_or("ReplaceObject part is outside the Setup")?;
                    part.0 = object_id;
                    scene.report.push(format!(
                        "Frame {frame_index}: part {part_index} → {object_id:08X}"
                    ));
                }
            }
        }
        scene.report.push(format!(
            "Animation {:08X}, frame {}/{} (forward)",
            animation.id,
            request.animation_frame,
            animation.frames.len()
        ));
    }
    if patches.len() > 256 {
        return Err("Preview palette patch limit exceeds 256".into());
    }
    let base_palette = scene.colors.clone();
    let mut patch_colors = Vec::new();
    let mut palette_bytes = 0_usize;
    for (id, offset, length) in patches {
        let colors = DatPalette::decode(&read(&mut archive, id)?)
            .map_err(|e| e.to_string())?
            .colors;
        palette_bytes += colors.len() * 4;
        if palette_bytes > 16 * 1024 * 1024 {
            return Err("Preview palette budget exceeds 16 MiB".into());
        }
        if scene.colors.is_empty() {
            scene.colors = colors.clone();
        }
        patch_colors.push((offset as usize, length as usize, colors));
    }
    let mut cache = BTreeMap::new();
    let mut texture_bytes = 0_usize;
    for (part, (id, scale, frame)) in models.into_iter().enumerate() {
        let gfx = GraphicsObject::decode(&read(&mut archive, id)?)
            .map_err(|e| format!("GfxObj {id:08X}: {e}"))?;
        scene.report.push(format!(
            "Part {part:02} · {id:08X} · {} vertices · {} polygons",
            gfx.vertices.len(),
            gfx.polygons.len()
        ));
        for polygon in gfx.polygons.values() {
            // Both authored polygon sides are resolved independently.
            let sides = if polygon.cull == 2 { 2 } else { 1 };
            for side in 0..sides {
                let surface = if side == 0 {
                    polygon.positive_surface
                } else {
                    polygon.negative_surface
                };
                if surface < 0 {
                    continue;
                }
                let surface_id = gfx.surfaces[surface as usize];
                let key = (part, surface_id);
                let texture = if let Some(&t) = cache.get(&key) {
                    t
                } else {
                    let mut s = DatSurface::decode(&read(&mut archive, surface_id)?)
                        .map_err(|e| e.to_string())?;
                    if let Some(&new) = replacements.get(&(part, s.texture)) {
                        s.texture = new;
                    }
                    let texture = if s.texture == 0 {
                        Texture {
                            width: 1,
                            height: 1,
                            pixels: vec![crate::preview_pixels::argb(s.color)],
                        }
                    } else {
                        let list = bace_dat::decode_texture_list(&read(&mut archive, s.texture)?)
                            .map_err(|e| e.to_string())?;
                        let id = list.iter().copied().find(|id|archive.records().contains_key(id)).ok_or_else(||format!("Texture list {:08X} has no available image in the selected Portal DAT: {list:08X?}",s.texture))?;
                        if list.first() != Some(&id) {
                            scene.report.push(format!("Texture {:08X}: using available mip {id:08X}; preferred image is absent",s.texture));
                        }
                        let t = DatTexture::decode(&read(&mut archive, id)?)
                            .map_err(|e| e.to_string())?;
                        let palette = if !base_palette.is_empty() {
                            base_palette.clone()
                        } else if let Some(id) = t
                            .palette
                            .filter(|id| *id != 0)
                            .or((s.palette != 0).then_some(s.palette))
                        {
                            DatPalette::decode(&read(&mut archive, id)?)
                                .map_err(|e| e.to_string())?
                                .colors
                        } else {
                            Vec::new()
                        };
                        if scene.colors.is_empty() {
                            scene.colors = palette.clone();
                        }
                        let mut palette = palette;
                        for (offset, len, colors) in
                            patch_colors.iter().filter(|_| matches!(t.format, 41 | 101))
                        {
                            let end = offset.checked_add(*len).ok_or("Palette range overflow")?;
                            if end > palette.len() || end > colors.len() {
                                return Err(
                                    "Palette replacement range exceeds source or destination"
                                        .into(),
                                );
                            }
                            palette[*offset..end].copy_from_slice(&colors[*offset..end]);
                        }
                        Texture {
                            width: t.width as usize,
                            height: t.height as usize,
                            pixels: crate::preview_pixels::pixels(&t, &palette)?,
                        }
                    };
                    texture_bytes += texture.pixels.len() * 4;
                    if texture_bytes > 64 * 1024 * 1024 {
                        return Err("Preview texture budget exceeds 64 MiB".into());
                    }
                    let index = scene.textures.len();
                    scene.textures.push(texture);
                    cache.insert(key, index);
                    index
                };
                let uvs = if side == 0 {
                    &polygon.positive_uvs
                } else {
                    &polygon.negative_uvs
                };
                for k in 1..polygon.vertices.len() - 1 {
                    let corners = if side == 0 {
                        [0, k, k + 1]
                    } else {
                        [0, k + 1, k]
                    };
                    let mut points = [[0.0; 3]; 3];
                    let mut coords = [[0.0; 2]; 3];
                    for (c, index) in corners.into_iter().enumerate() {
                        let vertex = &gfx.vertices[&polygon.vertices[index]];
                        points[c] = transform(vertex.position, scale, &frame)?;
                        if let Some(&uv) = uvs.get(index) {
                            coords[c] = vertex.uvs[usize::from(uv)];
                        }
                        for (axis, coordinate) in points[c].iter().copied().enumerate() {
                            scene.bounds.0[axis] = scene.bounds.0[axis].min(coordinate);
                            scene.bounds.1[axis] = scene.bounds.1[axis].max(coordinate);
                        }
                    }
                    if scene.triangles.len() >= 50_000 {
                        return Err("Preview triangle budget exceeds 50,000".into());
                    }
                    scene.triangles.push(Triangle {
                        points,
                        uvs: coords,
                        texture,
                        shade: 1.0,
                        cull: polygon.cull != 1,
                    });
                }
            }
        }
    }
    Ok(scene)
}
fn read(archive: &mut DatArchive, id: u32) -> Result<Vec<u8>, String> {
    archive.read(id).map_err(|e| format!("Asset {id:08X}: {e}"))
}
fn transform(p: [f32; 3], s: [f32; 3], f: &bace_dat::ModelFrame) -> Result<[f32; 3], String> {
    let [w, x, y, z] = f.rotation;
    let norm = w * w + x * x + y * y + z * z;
    if (norm - 1.0).abs() > 0.01 {
        return Err("Setup rotation is not a unit quaternion".into());
    }
    let [a, b, c] = [p[0] * s[0], p[1] * s[1], p[2] * s[2]];
    let out = [
        (1.0 - 2.0 * (y * y + z * z)) * a
            + 2.0 * (x * y - z * w) * b
            + 2.0 * (x * z + y * w) * c
            + f.origin[0],
        2.0 * (x * y + z * w) * a
            + (1.0 - 2.0 * (x * x + z * z)) * b
            + 2.0 * (y * z - x * w) * c
            + f.origin[1],
        2.0 * (x * z - y * w) * a
            + 2.0 * (y * z + x * w) * b
            + (1.0 - 2.0 * (x * x + y * y)) * c
            + f.origin[2],
    ];
    if out.iter().any(|v| !v.is_finite() || v.abs() > 1.0e7) {
        return Err("Model coordinate exceeds preview bounds".into());
    }
    Ok(out)
}
