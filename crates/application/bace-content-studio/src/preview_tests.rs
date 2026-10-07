use crate::preview_scene::{load,PreviewRequest};

#[test]
#[ignore = "Requires user-supplied client DAT; no proprietary assets committed"]
fn supplied_model_preview() {
    let directory=std::env::var_os("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY");
    for setup in [0x02000001] {
        let scene=load(PreviewRequest {path:std::path::PathBuf::from(&directory).join("client_portal.dat"),setup,..Default::default()}).expect("real Setup resolves");
        assert!(scene.triangles.len()>100);assert!(!scene.textures.is_empty());
        let image=crate::preview_render::render(&scene,Default::default()).expect("render");
        assert!(image.pixels.iter().filter(|p|**p!=eframe::egui::Color32::from_rgb(13,19,27)).count()>1000);
        eprintln!("Setup {setup:08X}: {} triangles / {} textures",scene.triangles.len(),scene.textures.len());
    }
}

#[test]
#[ignore = "Requires user-supplied DAT"]
fn supplied_clothing_palette_preview() {
    let path=std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory")).join("client_portal.dat");
    let mut dat=bace_dat::DatArchive::open(&path).unwrap();
    let ids:Vec<_>=dat.records().keys().copied().filter(|id|id>>24==16).collect();
    let mut checked=0;
    for id in ids {
        let table=bace_dat::ClothingTable::decode(&dat.read(id).unwrap()).unwrap();
        if !table.setups.contains_key(&0x02000001) || table.templates.is_empty() {continue;}
        let template=*table.templates.keys().next().unwrap();
        let scene=load(PreviewRequest{path:path.clone(),setup:0x02000001,clothing:id,template,shade:0.5,..Default::default()}).unwrap_or_else(|e|panic!("Clothing {id:08X}: {e}"));
        assert!(!scene.triangles.is_empty());
        eprintln!("Clothing {id:08X} template {template}: {} triangles",scene.triangles.len());
        checked+=1;if checked==5 {break;}
    }
    assert_eq!(checked,5);
}
