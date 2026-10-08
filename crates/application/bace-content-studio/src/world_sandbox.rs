//! Bounded offline terrain and pack-location inspection with a flying camera.
use bace_content::WorldRecordV1;
use bace_dat::{DatArchive, Landblock, LandblockInfo, RegionLand};
use bace_storage_codec::{PackKey, PackLimits, PackLookup, load_manifest};
use eframe::egui::{self, Color32, Pos2};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

#[derive(Clone)]
struct Marker {
    at: [f32; 3],
    label: String,
    color: Color32,
}

struct Terrain {
    landblock: u16,
    triangles: Vec<([[f32; 3]; 3], Color32)>,
    markers: Vec<Marker>,
}

struct LoadJob {
    receiver: Receiver<Result<Terrain, String>>,
    thread: JoinHandle<()>,
}

pub(crate) struct WorldSandbox {
    portal: Option<PathBuf>,
    cell: Option<PathBuf>,
    manifest: Option<PathBuf>,
    landblock: String,
    x: String,
    y: String,
    z: String,
    camera: [f32; 3],
    yaw: f32,
    pitch: f32,
    scene: Option<Terrain>,
    job: Option<LoadJob>,
    notice: String,
}

impl Default for WorldSandbox {
    fn default() -> Self {
        Self {
            portal: None,
            cell: None,
            manifest: None,
            landblock: "A260".into(),
            x: "96".into(),
            y: "96".into(),
            z: "100".into(),
            camera: [96.0, 96.0, 100.0],
            yaw: 0.0,
            pitch: -0.2,
            scene: None,
            job: None,
            notice: String::new(),
        }
    }
}

impl WorldSandbox {
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self
            .job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
        {
            let job = self.job.take().expect("finished world load");
            let panicked = job.thread.join().is_err();
            if panicked {
                self.notice = "World loader stopped unexpectedly".into();
            } else {
                match job.receiver.try_recv() {
                    Ok(Ok(scene)) => {
                        self.notice = format!(
                            "Landblock {:04X}: {} terrain triangles and {} location markers",
                            scene.landblock,
                            scene.triangles.len(),
                            scene.markers.len()
                        );
                        self.landblock = format!("{:04X}", scene.landblock);
                        self.scene = Some(scene);
                    }
                    Ok(Err(error)) => self.notice = error,
                    Err(error) => self.notice = error.to_string(),
                }
            }
        } else if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn request(&mut self, landblock: u16) {
        let (Some(portal), Some(cell)) = (self.portal.clone(), self.cell.clone()) else {
            self.notice = "Choose both client_portal.dat and client_cell_1.dat".into();
            return;
        };
        let manifest = self.manifest.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("studio-world-landblock".into())
            .spawn(move || {
                let result = load_terrain(&portal, &cell, manifest.as_deref(), landblock);
                let _ = sender.send(result);
            }) {
            Ok(thread) => {
                self.job = Some(LoadJob { receiver, thread });
                self.notice = format!("Loading landblock {landblock:04X}…");
            }
            Err(error) => self.notice = error.to_string(),
        }
    }

    fn teleport(&mut self) {
        let parsed = (|| -> Result<(u16, [f32; 3]), String> {
            let id = u16::from_str_radix(
                self.landblock
                    .trim()
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .map_err(|e| e.to_string())?;
            let coordinates = [
                self.x.parse::<f32>(),
                self.y.parse::<f32>(),
                self.z.parse::<f32>(),
            ];
            let [Ok(x), Ok(y), Ok(z)] = coordinates else {
                return Err("Enter numeric X, Y and Z".into());
            };
            if ![x, y, z].iter().all(|v| v.is_finite())
                || !(0.0..192.0).contains(&x)
                || !(0.0..192.0).contains(&y)
            {
                return Err("Local X and Y must be from 0 to 192; Z must be finite".into());
            }
            Ok((id, [x, y, z]))
        })();
        match parsed {
            Ok((landblock, camera)) => {
                self.camera = camera;
                self.request(landblock);
            }
            Err(error) => self.notice = error,
        }
    }

    fn fly(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if response.dragged() {
            let delta = ui.input(|input| input.pointer.delta());
            self.yaw += delta.x * 0.005;
            self.pitch = (self.pitch - delta.y * 0.005).clamp(-1.45, 1.45);
        }
        if !response.has_focus() || self.scene.is_none() {
            return;
        }
        let (dt, keys, fast) = ui.input(|input| {
            (
                input.stable_dt.min(0.1),
                [
                    egui::Key::W,
                    egui::Key::S,
                    egui::Key::A,
                    egui::Key::D,
                    egui::Key::Q,
                    egui::Key::E,
                ]
                .map(|key| input.key_down(key)),
                input.modifiers.shift,
            )
        });
        if !keys.contains(&true) {
            return;
        }
        let speed = if fast { 120.0 } else { 40.0 };
        let forward = [self.yaw.cos(), self.yaw.sin()];
        let right = [-self.yaw.sin(), self.yaw.cos()];
        let axial = (keys[0] as i32 - keys[1] as i32) as f32;
        let lateral = (keys[3] as i32 - keys[2] as i32) as f32;
        let vertical = (keys[5] as i32 - keys[4] as i32) as f32;
        self.camera[0] += (axial * forward[0] + lateral * right[0]) * speed * dt;
        self.camera[1] += (axial * forward[1] + lateral * right[1]) * speed * dt;
        self.camera[2] += vertical * speed * dt;
        ui.ctx().request_repaint();
        if self.job.is_none() {
            let Some(current) = self.scene.as_ref().map(|scene| scene.landblock) else {
                return;
            };
            let mut bx = i32::from(current >> 8);
            let mut by = i32::from(current & 255);
            while self.camera[0] < 0.0 {
                self.camera[0] += 192.0;
                bx -= 1;
            }
            while self.camera[0] >= 192.0 {
                self.camera[0] -= 192.0;
                bx += 1;
            }
            while self.camera[1] < 0.0 {
                self.camera[1] += 192.0;
                by -= 1;
            }
            while self.camera[1] >= 192.0 {
                self.camera[1] -= 192.0;
                by += 1;
            }
            if !(0..=255).contains(&bx) || !(0..=255).contains(&by) {
                self.notice = "World edge reached".into();
                self.camera[0] = self.camera[0].clamp(0.0, 191.99);
                self.camera[1] = self.camera[1].clamp(0.0, 191.99);
            } else {
                let next = ((bx as u16) << 8) | by as u16;
                if next != current {
                    self.scene = None;
                    self.request(next);
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Inspect actual DAT terrain and content locations with a flying camera. This view uses approximate terrain colors and location markers; it does not simulate collision or a stock client.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Portal DAT…").clicked() {
                self.portal = rfd::FileDialog::new()
                    .add_filter("DAT", &["dat"])
                    .pick_file();
            }
            ui.label(
                self.portal
                    .as_ref()
                    .map_or("No Portal DAT".into(), |path| path.display().to_string()),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Cell DAT…").clicked() {
                self.cell = rfd::FileDialog::new()
                    .add_filter("DAT", &["dat"])
                    .pick_file();
            }
            ui.label(
                self.cell
                    .as_ref()
                    .map_or("No Cell DAT".into(), |path| path.display().to_string()),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Pack manifest…").clicked() {
                self.manifest = rfd::FileDialog::new()
                    .add_filter("BACE generation", &["manifest"])
                    .pick_file();
            }
            ui.label(
                self.manifest
                    .as_ref()
                    .map_or("No pack: terrain only".into(), |path| {
                        path.display().to_string()
                    }),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Landblock");
            ui.add(egui::TextEdit::singleline(&mut self.landblock).desired_width(65.0));
            for (label, value) in [("X", &mut self.x), ("Y", &mut self.y), ("Z", &mut self.z)] {
                ui.label(label);
                ui.add(egui::TextEdit::singleline(value).desired_width(65.0));
            }
            if ui
                .add_enabled(self.job.is_none(), egui::Button::new("Teleport camera"))
                .clicked()
            {
                self.teleport();
            }
        });
        if self.job.is_some() {
            ui.spinner();
        }
        let size = egui::vec2(ui.available_width().min(850.0), 470.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        if response.clicked() {
            response.request_focus();
        }
        ui.painter()
            .rect_filled(rect, 4.0, Color32::from_rgb(18, 25, 36));
        self.fly(ui, &response);
        if let Some(scene) = &self.scene {
            draw(ui, rect, scene, self.camera, self.yaw, self.pitch);
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Choose DATs and teleport to a landblock",
                egui::FontId::proportional(17.0),
                Color32::LIGHT_GRAY,
            );
        }
        ui.small("Click viewport, drag to look · W/S forward/back · A/D strafe · Q/E down/up · Shift faster");
        ui.small(format!(
            "Camera: {:.1}, {:.1}, {:.1}",
            self.camera[0], self.camera[1], self.camera[2]
        ));
        crate::theme::notice(ui, &self.notice);
        if let Some(scene) = &self.scene {
            egui::CollapsingHeader::new(format!("{} content/static markers", scene.markers.len()))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for marker in scene.markers.iter().take(200) {
                                ui.label(format!(
                                    "{} at {:.1}, {:.1}, {:.1}",
                                    marker.label, marker.at[0], marker.at[1], marker.at[2]
                                ));
                            }
                        });
                });
        }
    }
}

fn load_terrain(
    portal_path: &PathBuf,
    cell_path: &PathBuf,
    manifest_path: Option<&std::path::Path>,
    landblock: u16,
) -> Result<Terrain, String> {
    let mut portal = DatArchive::open(portal_path).map_err(|e| e.to_string())?;
    let mut cell = DatArchive::open(cell_path).map_err(|e| e.to_string())?;
    if portal.header().dataset != 1 || cell.header().dataset != 2 {
        return Err("Choose a Portal DAT and a Cell DAT".into());
    }
    let land = RegionLand::decode_prefix(&portal.read(0x13000000).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if land.square_length != 24.0 || land.landblock_length != 8 || land.vertices_per_cell != 1 {
        return Err("Unsupported DAT terrain lattice".into());
    }
    let base = u32::from(landblock) << 16;
    let block = Landblock::decode(&cell.read(base | 0xffff).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if block.id != (base | 0xffff) {
        return Err("Landblock DAT identity mismatch".into());
    }
    let mut triangles = Vec::with_capacity(128);
    let gx = u32::from(landblock >> 8) * 8;
    let gy = u32::from(landblock & 255) * 8;
    for x in 0..8u32 {
        for y in 0..8u32 {
            let point = |dx: u32, dy: u32| {
                let index = ((x + dx) * 9 + y + dy) as usize;
                [
                    ((x + dx) as f32) * 24.0,
                    ((y + dy) as f32) * 24.0,
                    land.heights[usize::from(block.heights[index])],
                ]
            };
            let (a, b, c, d) = (point(0, 0), point(1, 0), point(0, 1), point(1, 1));
            let magic_a = (gx + x).wrapping_mul(214614067).wrapping_add(1813693831);
            let magic_b = (gx + x).wrapping_mul(1109124029);
            let split = (gy + y)
                .wrapping_mul(magic_a)
                .wrapping_sub(magic_b)
                .wrapping_sub(1369149221);
            let faces = if f64::from(split) * 2.3283064e-10 < 0.5 {
                [[a, b, c], [d, c, b]]
            } else {
                [[a, b, d], [a, d, c]]
            };
            let terrain = block.terrain[(x * 9 + y) as usize];
            let shade = ((terrain & 31) as u8).saturating_mul(3);
            let color = Color32::from_rgb(45 + shade, 85 + shade, 48 + shade / 2);
            for face in faces {
                triangles.push((face, color));
            }
        }
    }
    let mut markers = Vec::new();
    if block.has_objects {
        let info = LandblockInfo::decode(&cell.read(base | 0xfffe).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for object in info.objects.iter().take(4096) {
            markers.push(Marker {
                at: object.frame.origin,
                label: format!("DAT static {:08X}", object.id),
                color: Color32::from_rgb(175, 190, 215),
            });
        }
    }
    if let Some(path) = manifest_path {
        let manifest = load_manifest(path, PackLimits::default()).map_err(|e| e.to_string())?;
        let generation = manifest
            .open(
                path.parent().ok_or("Pack manifest has no parent")?,
                PackLimits::default(),
            )
            .map_err(|e| e.to_string())?;
        if let PackLookup::Record(index) = generation
            .lookup(PackKey {
                namespace: 2,
                id: u64::from(landblock),
            })
            .map_err(|e| e.to_string())?
        {
            let index = bace_content_tools::decode_landblock_index(index.bytes())?;
            if index.instance_ids.len() + index.encounter_ids.len() > 4096 {
                return Err("Landblock marker limit".into());
            }
            for id in index.instance_ids {
                let PackLookup::Record(row) = generation
                    .lookup(PackKey {
                        namespace: 20,
                        id: u64::from(id),
                    })
                    .map_err(|e| e.to_string())?
                else {
                    return Err("Missing indexed world instance".into());
                };
                let WorldRecordV1::LandblockInstance(row) =
                    bace_content_tools::decode_world_record(row.bytes())?
                else {
                    return Err("Wrong indexed instance type".into());
                };
                markers.push(Marker {
                    at: [row.origin_x, row.origin_y, row.origin_z],
                    label: format!("Instance {:08X} · weenie {}", row.guid, row.weenie_class_id),
                    color: Color32::YELLOW,
                });
            }
            for id in index.encounter_ids {
                let PackLookup::Record(row) = generation
                    .lookup(PackKey {
                        namespace: 17,
                        id: u64::from(id),
                    })
                    .map_err(|e| e.to_string())?
                else {
                    return Err("Missing indexed encounter".into());
                };
                let WorldRecordV1::Encounter(row) =
                    bace_content_tools::decode_world_record(row.bytes())?
                else {
                    return Err("Wrong indexed encounter type".into());
                };
                let x = row.cell_x as f32 * 24.0 + 12.0;
                let y = row.cell_y as f32 * 24.0 + 12.0;
                markers.push(Marker {
                    at: [x, y, 0.0],
                    label: format!("Encounter {} · weenie {}", row.id, row.weenie_class_id),
                    color: Color32::LIGHT_RED,
                });
            }
        }
    }
    Ok(Terrain {
        landblock,
        triangles,
        markers,
    })
}

fn draw(ui: &egui::Ui, rect: egui::Rect, scene: &Terrain, camera: [f32; 3], yaw: f32, pitch: f32) {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let forward = [cy * cp, sy * cp, sp];
    let right = [-sy, cy, 0.0];
    let up = [-cy * sp, -sy * sp, cp];
    let focal = rect.width().min(rect.height()) * 0.85;
    let project = |point: [f32; 3]| -> Option<(Pos2, f32)> {
        let offset = [
            point[0] - camera[0],
            point[1] - camera[1],
            point[2] - camera[2],
        ];
        let dot = |basis: [f32; 3]| offset.iter().zip(basis).map(|(a, b)| a * b).sum::<f32>();
        let depth = dot(forward);
        if depth <= 0.5 {
            return None;
        }
        Some((
            Pos2::new(
                rect.center().x + dot(right) * focal / depth,
                rect.center().y - dot(up) * focal / depth,
            ),
            depth,
        ))
    };
    let mut faces = scene
        .triangles
        .iter()
        .filter_map(|(triangle, color)| {
            let points = triangle.map(project);
            let [Some(a), Some(b), Some(c)] = points else {
                return None;
            };
            Some(((a.1 + b.1 + c.1) / 3.0, [a.0, b.0, c.0], *color))
        })
        .collect::<Vec<_>>();
    faces.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, points, color) in faces {
        ui.painter().add(egui::Shape::convex_polygon(
            points.to_vec(),
            color,
            egui::Stroke::NONE,
        ));
    }
    for marker in &scene.markers {
        if let Some((point, depth)) = project(marker.at)
            && rect.contains(point)
        {
            ui.painter()
                .circle_filled(point, (110.0 / depth).clamp(3.0, 8.0), marker.color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::load_terrain;

    #[test]
    #[ignore = "Requires user-supplied Portal and Cell DATs; no proprietary assets committed"]
    fn supplied_landblock_loads_for_offline_flight() {
        let directory = std::path::PathBuf::from(
            std::env::var_os("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY"),
        );
        let scene = load_terrain(
            &directory.join("client_portal.dat"),
            &directory.join("client_cell_1.dat"),
            None,
            0xa260,
        )
        .unwrap();
        assert_eq!(scene.triangles.len(), 128);
    }
}
