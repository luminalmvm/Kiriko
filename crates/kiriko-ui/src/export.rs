//! Export v0 (docs/06-RENDER-PIPELINE.md §7): render every comp frame through
//! the compositor at full resolution and encode to H.264/mp4.
//!
//! In plain terms: the same pixels the Viewer shows, written to a file — the
//! preview-equals-export promise (K-031) holds because this path reuses the
//! identical colour engine and compositor. Runs entirely on its own thread
//! with its own decoders (K-017: the UI never waits); progress streams back,
//! and cancel is a flag the loop checks every frame. The export queue,
//! presets, and hardware encoders replace this single-shot v0.

#![cfg(feature = "media")]

use kiriko_core::model::{Document, LayerKind, MatteChannel, ProjectItem};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use uuid::Uuid;

pub enum ExportEvent {
    Progress { frame: usize, total: usize },
    Done(PathBuf),
    Failed(String),
}

pub struct ExportHandle {
    pub events: Receiver<ExportEvent>,
    cancel: Arc<AtomicBool>,
}

impl ExportHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Everything the export thread needs about one footage item.
#[derive(Clone)]
pub struct ItemInfo {
    pub path: PathBuf,
    pub fps: f64,
    pub frames: usize,
}

#[allow(clippy::too_many_arguments)]
pub fn start(
    doc: Arc<Document>,
    comp_id: Uuid,
    items: HashMap<Uuid, ItemInfo>,
    gpu: kiriko_gpu::GpuContext,
    out_path: PathBuf,
    bit_rate: Option<i64>,
) -> ExportHandle {
    let (tx, events) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        let result = run(&doc, comp_id, &items, &gpu, &out_path, bit_rate, &tx, &flag);
        let _ = match result {
            Ok(()) if flag.load(Ordering::Relaxed) => {
                let _ = std::fs::remove_file(&out_path); // no half files
                tx.send(ExportEvent::Failed("cancelled".into()))
            }
            Ok(()) => tx.send(ExportEvent::Done(out_path)),
            Err(e) => {
                let _ = std::fs::remove_file(&out_path);
                tx.send(ExportEvent::Failed(e))
            }
        };
    });
    ExportHandle { events, cancel }
}

#[allow(clippy::too_many_arguments)]
fn run(
    doc: &Document,
    comp_id: Uuid,
    items: &HashMap<Uuid, ItemInfo>,
    gpu: &kiriko_gpu::GpuContext,
    out_path: &std::path::Path,
    bit_rate: Option<i64>,
    tx: &Sender<ExportEvent>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let comp = doc.comp(comp_id).ok_or("composition missing")?;
    let fps = comp.frame_rate.fps().max(1.0);
    let total = (comp.duration.0.to_f64() * fps).round().max(1.0) as usize;

    let colour = kiriko_gpu::ColourEngine::new(gpu);
    let compositor = kiriko_gpu::Compositor::new(gpu);
    let mut encoder = kiriko_media::Encoder::open_with_bitrate(
        out_path,
        comp.width,
        comp.height,
        i32::try_from(comp.frame_rate.fps().round() as i64).unwrap_or(60),
        1,
        bit_rate,
    )
    .map_err(|e| e.to_string())?;

    let mut decoders: HashMap<Uuid, kiriko_media::VideoDecoder> = HashMap::new();

    for frame_n in 0..total {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let t = frame_n as f64 / fps;

        // Layers needed at t: visible ones plus their matte sources.
        let mut wanted: Vec<Uuid> = Vec::new();
        for l in &comp.layers {
            let in_span = t >= l.in_point.0.to_f64() && t < l.out_point.0.to_f64();
            if l.switches.visible && in_span {
                wanted.push(l.id);
                if let Some(m) = &l.matte {
                    if !wanted.contains(&m.layer) {
                        wanted.push(m.layer);
                    }
                }
            }
        }

        // Decode every needed layer's source frame (full resolution).
        let mut pixels: HashMap<Uuid, kiriko_media::DecodedFrame> = HashMap::new();
        for l in &comp.layers {
            if !wanted.contains(&l.id) {
                continue;
            }
            if t < l.in_point.0.to_f64() || t >= l.out_point.0.to_f64() {
                continue;
            }
            let item = match &l.kind {
                LayerKind::Footage { item } => item,
                LayerKind::Solid { colour } => {
                    let px = solid_rgba(*colour);
                    pixels.insert(
                        l.id,
                        kiriko_media::DecodedFrame {
                            width: comp.width,
                            height: comp.height,
                            rgba: px_tile(&px, comp.width, comp.height),
                        },
                    );
                    continue;
                }
            };
            let Some(info) = items.get(item) else {
                continue;
            };
            let lt = t - l.start_offset.0.to_f64();
            let source_frame =
                ((lt * info.fps).round().max(0.0) as usize).min(info.frames.saturating_sub(1));
            if !decoders.contains_key(item) {
                let index = kiriko_media::index::build_frame_index(&info.path)
                    .map_err(|e| e.to_string())?;
                let dec = kiriko_media::VideoDecoder::open(&info.path, index)
                    .map_err(|e| e.to_string())?;
                decoders.insert(*item, dec);
            }
            let dec = decoders.get_mut(item).ok_or("decoder missing")?;
            let px = dec
                .frame_rgba(source_frame, None)
                .map_err(|e| e.to_string())?;
            pixels.insert(l.id, px);
        }

        // Linearised textures + matte textures, then composite bottom-up.
        // (Mirror of the preview path; consolidated into the evaluator later.)
        let linear: HashMap<Uuid, egui_wgpu::wgpu::Texture> = pixels
            .iter()
            .map(|(id, px)| {
                let src = colour.upload_srgb8(gpu, &px.rgba, px.width, px.height);
                (*id, colour.linearise(gpu, &src))
            })
            .collect();

        let mut draws: Vec<kiriko_gpu::CompositeLayer> = Vec::new();
        let mut matte_textures: Vec<(Uuid, egui_wgpu::wgpu::Texture)> = Vec::new();
        for l in comp.layers.iter().rev() {
            if !l.switches.visible || !linear.contains_key(&l.id) {
                continue;
            }
            if let Some(mr) = &l.matte {
                if let (Some(src_layer), Some(mtex)) = (
                    comp.layers.iter().find(|x| x.id == mr.layer),
                    linear.get(&mr.layer),
                ) {
                    let mlt = t - src_layer.start_offset.0.to_f64();
                    let mtr = &src_layer.transform;
                    let mp = &pixels[&mr.layer];
                    let rendered = compositor.composite(
                        gpu,
                        comp.width,
                        comp.height,
                        [0.0, 0.0, 0.0, 0.0],
                        &[kiriko_gpu::CompositeLayer {
                            texture: mtex,
                            size: (mp.width as f32, mp.height as f32),
                            position: (
                                mtr.position_x.value_at(mlt) as f32,
                                mtr.position_y.value_at(mlt) as f32,
                            ),
                            anchor: (
                                mtr.anchor_x.value_at(mlt) as f32,
                                mtr.anchor_y.value_at(mlt) as f32,
                            ),
                            scale: (
                                mtr.scale_x.value_at(mlt) as f32,
                                mtr.scale_y.value_at(mlt) as f32,
                            ),
                            rotation_deg: mtr.rotation.value_at(mlt) as f32,
                            opacity: mtr.opacity.value_at(mlt) as f32,
                            matte: None,
                            blend: kiriko_gpu::Blend::Normal,
                        }],
                    );
                    matte_textures.push((l.id, rendered));
                }
            }
        }
        for l in comp.layers.iter().rev() {
            if !l.switches.visible {
                continue;
            }
            let Some(tex) = linear.get(&l.id) else {
                continue;
            };
            let px = &pixels[&l.id];
            let lt = t - l.start_offset.0.to_f64();
            let tr = &l.transform;
            draws.push(kiriko_gpu::CompositeLayer {
                texture: tex,
                size: (px.width as f32, px.height as f32),
                position: (
                    tr.position_x.value_at(lt) as f32,
                    tr.position_y.value_at(lt) as f32,
                ),
                anchor: (
                    tr.anchor_x.value_at(lt) as f32,
                    tr.anchor_y.value_at(lt) as f32,
                ),
                scale: (
                    tr.scale_x.value_at(lt) as f32,
                    tr.scale_y.value_at(lt) as f32,
                ),
                rotation_deg: tr.rotation.value_at(lt) as f32,
                opacity: tr.opacity.value_at(lt) as f32,
                matte: l.matte.as_ref().and_then(|mr| {
                    matte_textures
                        .iter()
                        .find(|(id, _)| *id == l.id)
                        .map(|(_, mtex)| kiriko_gpu::MatteInput {
                            texture: mtex,
                            luma: matches!(mr.channel, MatteChannel::Luma),
                            inverted: mr.inverted,
                        })
                }),
                blend: match l.blend {
                    kiriko_core::model::BlendMode::Normal => kiriko_gpu::Blend::Normal,
                    kiriko_core::model::BlendMode::Add => kiriko_gpu::Blend::Add,
                    kiriko_core::model::BlendMode::Multiply => kiriko_gpu::Blend::Multiply,
                },
            });
        }

        let bg = comp.background.0;
        let linear_frame = compositor.composite(
            gpu,
            comp.width,
            comp.height,
            [
                f64::from(bg[0]),
                f64::from(bg[1]),
                f64::from(bg[2]),
                f64::from(bg[3]),
            ],
            &draws,
        );
        let shown = colour.display(gpu, &linear_frame);
        let rgba = colour.readback8(gpu, &shown).map_err(|e| e.to_string())?;
        encoder.write_rgba(&rgba).map_err(|e| e.to_string())?;

        let _ = tx.send(ExportEvent::Progress {
            frame: frame_n + 1,
            total,
        });
    }
    encoder.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn srgb_encode(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let e = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0).round() as u8
}

pub fn solid_rgba(c: kiriko_core::model::LinearColour) -> [u8; 4] {
    [
        srgb_encode(c.0[0]),
        srgb_encode(c.0[1]),
        srgb_encode(c.0[2]),
        (c.0[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

pub fn px_tile(px: &[u8; 4], w: u32, h: u32) -> Vec<u8> {
    std::iter::repeat_n(*px, (w * h) as usize)
        .flatten()
        .collect()
}

/// Collect the ItemInfo map from probed media (UI thread, cheap).
pub fn item_infos(
    doc: &Document,
    media: &crate::app_state::media::MediaRegistry,
) -> HashMap<Uuid, ItemInfo> {
    let mut map = HashMap::new();
    for item in &doc.items {
        if let ProjectItem::Footage(f) = item {
            if let Some(crate::app_state::media::MediaStatus::Ready { probe, frames, .. }) =
                media.map.get(&f.id)
            {
                if let Some(v) = &probe.video {
                    map.insert(
                        f.id,
                        ItemInfo {
                            path: PathBuf::from(&f.media.absolute_path),
                            fps: v.fps(),
                            frames: *frames,
                        },
                    );
                }
            }
        }
    }
    map
}
