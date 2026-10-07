use crate::{
    preview_render::{self, Camera},
    preview_scene::{self, PreviewRequest, PreviewScene},
    theme,
};
use eframe::egui;
use std::{path::PathBuf, sync::mpsc, thread::JoinHandle};

pub(crate) struct Preview {
    path: Option<PathBuf>,
    setup: String,
    clothing: String,
    palette: String,
    template: u32,
    shade: f64,
    scene: Option<PreviewScene>,
    job: Option<LoadJob>,
    notice: String,
    texture: Option<egui::TextureHandle>,
    camera: Camera,
    filter: String,
    kind: usize,
    apply: Option<(u32, u32, u32)>,
}
struct LoadJob {
    receiver: mpsc::Receiver<Result<PreviewScene, String>>,
    thread: JoinHandle<()>,
}
impl Default for Preview {
    fn default() -> Self {
        Self {
            path: None,
            setup: "02000001".into(),
            clothing: "0".into(),
            palette: "0".into(),
            template: 1,
            shade: 0.0,
            scene: None,
            job: None,
            notice: String::new(),
            texture: None,
            camera: Camera::default(),
            filter: String::new(),
            kind: 0,
            apply: None,
        }
    }
}
impl Preview {
    pub fn take_apply(&mut self) -> Option<(u32, u32, u32)> {
        self.apply.take()
    }
    fn load(&mut self, ctx: &egui::Context, appearance: Option<bace_content::WeenieV1>) {
        let Some(path) = self.path.clone() else {
            self.notice = "Choose your client_portal.dat first.".into();
            return;
        };
        let request = (|| {
            Ok::<_, String>(PreviewRequest {
                path,
                setup: did(&self.setup)?,
                clothing: did(&self.clothing)?,
                palette: did(&self.palette)?,
                template: self.template,
                shade: self.shade,
                appearance,
            })
        })();
        let request = match request {
            Ok(r) => r,
            Err(e) => {
                self.notice = e;
                return;
            }
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("studio-dat-preview".into())
            .spawn(move || {
                let _ = sender.send(preview_scene::load(request));
                ctx.request_repaint();
            }) {
            Ok(thread) => {
                self.job = Some(LoadJob { receiver, thread });
                self.notice = "Loading selected assets…".into();
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
    fn raster(&mut self, ctx: &egui::Context) {
        if let Some(scene) = &self.scene {
            match preview_render::render(scene, self.camera) {
                Ok(image) => {
                    if let Some(texture) = &mut self.texture {
                        texture.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        self.texture = Some(ctx.load_texture(
                            "model-preview",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                Err(e) => self.notice = e,
            }
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, document: Option<&toml::Value>) {
        let ctx = ui.ctx().clone();
        if self.job.as_ref().is_some_and(|j| j.thread.is_finished()) {
            if let Some(job) = self.job.take() {
                let joined = job.thread.join();
                match job.receiver.try_recv() {
                    Ok(Ok(scene)) if joined.is_ok() => {
                        self.notice = format!(
                            "Loaded {} triangles · {} textures",
                            scene.triangles.len(),
                            scene.textures.len()
                        );
                        self.scene = Some(scene);
                        self.camera = Camera::default();
                        self.raster(&ctx);
                    }
                    Ok(Err(e)) => {
                        self.notice = format!("Could not load preview: {e}");
                        self.scene = None;
                        self.texture = None;
                    }
                    _ => self.notice = "DAT worker stopped unexpectedly".into(),
                }
            }
        } else if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.job.is_none(),
                    egui::Button::new("Choose client_portal.dat…"),
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Asheron's Call DAT", &["dat"])
                    .pick_file()
            {
                self.path = Some(path);
                self.scene = None;
                self.texture = None;
            }
            theme::subtitle(
                ui,
                &self
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| {
                        "Select your installed client assets to enable preview".into()
                    }),
            );
        });
        ui.add_enabled_ui(self.job.is_none(),|ui| {
            ui.horizontal_wrapped(|ui| {
                for (label,value) in [("Setup / GfxObj",&mut self.setup),("ClothingBase",&mut self.clothing),("Palette / Set",&mut self.palette)] {
                    ui.label(label);ui.add(egui::TextEdit::singleline(value).desired_width(95.0).font(egui::TextStyle::Monospace).char_limit(10));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Palette template");ui.add(egui::DragValue::new(&mut self.template).range(0..=u32::MAX));
                ui.add(egui::Slider::new(&mut self.shade,0.0..=1.0).text("Shade"));
                if ui.add(theme::primary("Load preview")).clicked() {self.load(&ctx,None);}
                if ui.add_enabled(document.is_some(),egui::Button::new("From weenie")).clicked() && let Some(document)=document {
                    match document.clone().try_into::<bace_content::WeenieV1>() {
                        Ok(weenie)=>{
                            let data=|id|weenie.properties.data_ids.iter().find(|p|p.id==id).map_or(0,|p|p.value);
                            self.setup=format!("{:08X}",data(1));self.clothing=format!("{:08X}",data(7));self.palette=format!("{:08X}",data(6));
                            self.template=weenie.properties.ints.iter().find(|p|p.id==3).map_or(1,|p|p.value.max(0) as u32);
                            self.shade=weenie.properties.floats.iter().find(|p|p.id==12).map_or(0.0,|p|p.value);
                            self.load(&ctx,Some(weenie));
                        },Err(e)=>self.notice=e.to_string(),
                    }
                }
                if ui.add_enabled(document.is_some(),egui::Button::new("Use DIDs in weenie")).on_hover_text("Updates Setup, ClothingBase and Palette properties. Save the document to persist changes.").clicked() {
                    match (did(&self.setup),did(&self.clothing),did(&self.palette)) {
                        (Ok(setup),Ok(clothing),Ok(palette)) if setup>>24==2 && (clothing==0||clothing>>24==16) && (palette==0||palette>>24==4)=>self.apply=Some((setup,clothing,palette)),
                        _=>self.notice="To apply, choose a Setup (02), ClothingBase (10 or 0), and Palette (04 or 0).".into(),
                    }
                }
            });
        });
        if self.job.is_some() {
            ui.spinner();
        }
        ui.add_space(10.0);
        ui.columns(2,|columns| {
            let ui=&mut columns[0];
            ui.horizontal(|ui|{theme::eyebrow(ui,"MODEL PREVIEW");if ui.small_button("Center view").clicked(){self.camera=Camera::default();self.raster(&ctx);}});
            let side=ui.available_width().min(560.0);let(rect,response)=ui.allocate_exact_size(egui::vec2(side,side),egui::Sense::click_and_drag());
            ui.painter().rect_filled(rect,8,egui::Color32::from_rgb(13,19,27));
            if let Some(texture)=&self.texture {ui.painter().image(texture.id(),rect,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),egui::Color32::WHITE);} else {
                ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Choose a DAT and load a Setup DID",egui::FontId::proportional(15.0),theme::MUTED);
            }
            let delta=ui.input(|i|i.pointer.delta());let mut changed=false;
            if response.dragged_by(egui::PointerButton::Primary) {self.camera.yaw+=delta.x*0.01;self.camera.pitch=(self.camera.pitch+delta.y*0.01).clamp(-1.5,1.5);changed=true;}
            if response.dragged_by(egui::PointerButton::Secondary) {self.camera.pan[0]+=delta.x*512.0/side;self.camera.pan[1]+=delta.y*512.0/side;changed=true;}
            if response.hovered() {let scroll=ui.input(|i|i.smooth_scroll_delta.y);if scroll!=0.0 {self.camera.zoom=(self.camera.zoom*(scroll*0.002).exp()).clamp(0.2,4.0);changed=true;}}
            if response.double_clicked() {self.camera=Camera::default();changed=true;}
            if changed {self.raster(&ctx);}
            theme::subtitle(ui,"Drag to orbit · Right-drag to pan · Scroll to zoom");
            ui.small("Static placement preview. Animation, particles and translucent blending are not simulated.");
            let ui=&mut columns[1];theme::eyebrow(ui,"ASSET LOOKUP");
            ui.horizontal_wrapped(|ui|{for (index,label) in ["Setups","Clothing","Palettes","Palette sets","GfxObjs"].iter().enumerate(){ui.selectable_value(&mut self.kind,index,*label);}});
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter hexadecimal DID…"));
            let mut selected=None;
            egui::ScrollArea::vertical().id_salt("did-list").max_height(170.0).show(ui,|ui|{
                if let Some(scene)=&self.scene {
                    let prefix=[2,16,4,15,1][self.kind];let filter=self.filter.trim_start_matches("0x").to_ascii_uppercase();
                    let matches:Vec<_>=scene.ids.iter().filter(|id|*id>>24==prefix&&format!("{id:08X}").contains(&filter)).collect();
                    ui.small(format!("{} matches · showing up to 100",matches.len()));
                    for &&id in matches.iter().take(100) {if ui.button(format!("{id:08X}")).clicked(){selected=Some(id);}}
                } else {theme::subtitle(ui,"Load a preview to browse the DAT index.");}
            });
            if let Some(id)=selected {let text=format!("{id:08X}");match self.kind {0|4=>self.setup=text,1=>self.clothing=text,_=>self.palette=text}}
            ui.separator();theme::eyebrow(ui,"PALETTE COLORS");
            if let Some(scene)=&self.scene {
                let cell=(ui.available_width()/16.0).min(22.0);
                egui::ScrollArea::vertical().id_salt("swatches").max_height(140.0).show(ui,|ui|{
                    for (row,colors) in scene.colors.chunks(16).enumerate() {ui.horizontal(|ui|{ui.spacing_mut().item_spacing.x=1.0;for (col,&color) in colors.iter().enumerate(){let(r,response)=ui.allocate_exact_size(egui::vec2(cell,cell),egui::Sense::hover());let [r8,g,b,a]=crate::preview_pixels::argb(color);ui.painter().rect_filled(r,2,egui::Color32::from_rgba_unmultiplied(r8,g,b,a));response.on_hover_text(format!("Index {} · ARGB {color:08X}",row*16+col));}});}
                });
                ui.separator();theme::eyebrow(ui,"RESOLVED REFERENCES");
                egui::ScrollArea::both().id_salt("asset-report").max_height(200.0).show(ui,|ui|{for line in &scene.report {ui.monospace(line);}});
            }
        });
        theme::notice(ui, &self.notice);
    }
}
fn did(text: &str) -> Result<u32, String> {
    u32::from_str_radix(
        text.trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X"),
        16,
    )
    .map_err(|_| "Enter a hexadecimal DID, for example 02000001; use 0 for none.".into())
}
impl Drop for Preview {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            let _ = job.thread.join();
        }
    }
}
