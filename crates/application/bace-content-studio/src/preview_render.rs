use crate::preview_scene::PreviewScene;
use eframe::egui::{Color32, ColorImage};

#[derive(Clone, Copy)]
pub(crate) struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: [f32; 2],
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.4,
            pitch: 0.15,
            zoom: 1.0,
            pan: [0.0; 2],
        }
    }
}
/// Orthographic inspection rasterizer with a depth buffer. No runtime state or I/O.
pub(crate) fn render(scene: &PreviewScene, camera: Camera) -> Result<ColorImage, String> {
    const SIZE: usize = 512;
    let mut image = ColorImage::filled([SIZE, SIZE], Color32::from_rgb(13, 19, 27));
    if scene.triangles.is_empty() {
        return Ok(image);
    }
    let center = std::array::from_fn::<_, 3, _>(|i| (scene.bounds.0[i] + scene.bounds.1[i]) * 0.5);
    let extent = (0..3)
        .map(|i| (scene.bounds.1[i] - scene.bounds.0[i]).powi(2))
        .sum::<f32>()
        .sqrt()
        .max(0.01);
    let factor = SIZE as f32 * 0.82 / extent * camera.zoom;
    let (sy, cy) = camera.yaw.sin_cos();
    let (sp, cp) = camera.pitch.sin_cos();
    let project = |p: [f32; 3]| {
        let x = p[0] - center[0];
        let y = p[1] - center[1];
        let z = p[2] - center[2];
        let horizontal = x * cy - y * sy;
        let depth = x * sy + y * cy;
        [
            SIZE as f32 * 0.5 + horizontal * factor + camera.pan[0],
            SIZE as f32 * 0.5 - (z * cp - depth * sp) * factor + camera.pan[1],
            depth * cp + z * sp,
        ]
    };
    let mut depths = vec![f32::INFINITY; SIZE * SIZE];
    let mut work = 0_usize;
    for triangle in &scene.triangles {
        let p = triangle.points.map(project);
        let edge = |a: [f32; 3], b: [f32; 3], x: f32, y: f32| {
            (x - a[0]) * (b[1] - a[1]) - (y - a[1]) * (b[0] - a[0])
        };
        let area = edge(p[0], p[1], p[2][0], p[2][1]);
        if area.abs() < 0.001 {
            continue;
        }
        let min_x = p
            .iter()
            .map(|p| p[0])
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_x = p
            .iter()
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .clamp(0.0, SIZE as f32) as usize;
        let min_y = p
            .iter()
            .map(|p| p[1])
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_y = p
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .clamp(0.0, SIZE as f32) as usize;
        work += max_x.saturating_sub(min_x) * max_y.saturating_sub(min_y);
        if work > 32_000_000 {
            return Err(
                "Preview raster budget exceeded; zoom out or choose a smaller model.".into(),
            );
        }
        let a = std::array::from_fn::<_, 3, _>(|i| p[1][i] - p[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| p[2][i] - p[0][i]);
        let normal = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let norm = normal.iter().map(|n| n * n).sum::<f32>().sqrt().max(0.001);
        let lighting = (0.48
            + 0.52 * ((normal[0] * 0.3 - normal[1] * 0.4 + normal[2] * 0.85) / norm).abs())
            * triangle.shade;
        let texture = &scene.textures[triangle.texture];
        for y in min_y..max_y {
            for x in min_x..max_x {
                let x_f = x as f32 + 0.5;
                let y_f = y as f32 + 0.5;
                let weights = [
                    edge(p[1], p[2], x_f, y_f) / area,
                    edge(p[2], p[0], x_f, y_f) / area,
                    edge(p[0], p[1], x_f, y_f) / area,
                ];
                if weights.iter().any(|w| *w < -0.0001) {
                    continue;
                }
                let z = (0..3).map(|i| weights[i] * p[i][2]).sum::<f32>();
                let index = y * SIZE + x;
                if z >= depths[index] {
                    continue;
                }
                let uv = std::array::from_fn::<_, 2, _>(|j| {
                    (0..3)
                        .map(|i| weights[i] * triangle.uvs[i][j])
                        .sum::<f32>()
                        .rem_euclid(1.0)
                });
                let tx = ((uv[0] * texture.width as f32) as usize).min(texture.width - 1);
                let ty = ((uv[1] * texture.height as f32) as usize).min(texture.height - 1);
                let color = texture.pixels[ty * texture.width + tx];
                if color[3] < 128 {
                    continue;
                }
                image.pixels[index] = Color32::from_rgb(
                    (f32::from(color[0]) * lighting).min(255.0) as u8,
                    (f32::from(color[1]) * lighting).min(255.0) as u8,
                    (f32::from(color[2]) * lighting).min(255.0) as u8,
                );
                depths[index] = z;
            }
        }
    }
    Ok(image)
}
