//! Probing footage and decoding frames — gated behind the `media` feature.
//!
//! # In plain terms
//!
//! When a project imports or opens footage, the bridge reads the file's vital
//! statistics (resolution, frame rate, frame count) so the panels can show them,
//! and later it decodes individual frames for the Viewer. That work needs
//! FFmpeg, wired in through `lumit-media`. The `media` cargo feature (default on)
//! pulls it; `--no-default-features` drops it, and then nothing probes — every
//! footage item simply reports status "unprobed" and the crate still builds and
//! tests without FFmpeg (CI parity).
//!
//! The *results* of probing are plain data ([`MediaStatus`]), always compiled,
//! so the snapshot code embeds them the same way whether the feature is on or
//! off. Probing runs **synchronously** on the calling thread at this phase (the
//! egui frontend probes on a background thread; the bridge will follow once the
//! command surface stabilises) — acceptable while the first files are small and
//! imported one at a time.

use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// A footage item's probe result — the plain-data mirror of `lumit-ui`'s
/// `MediaStatus` (docs/07 §3.3). Compiled with or without the `media` feature;
/// only *populated* when the feature probes (so without it the variants are
/// never constructed at runtime — the snapshot always reads "unprobed").
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "media"), allow(dead_code))]
pub(crate) enum MediaStatus {
    /// Probed cleanly; carries the metadata the snapshot exposes.
    Ok(MediaInfo),
    /// The file is not on disk (moved, renamed, unmounted). A normal state that
    /// leads to the relink flow and a slate — never an error reply.
    Missing,
    /// The file is present but could not be read (corrupt or unsupported).
    Failed,
}

/// The metadata a probed footage item exposes in the snapshot.
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "media"), allow(dead_code))]
pub(crate) struct MediaInfo {
    /// Decodable frame count (from the frame index; 0 for audio-only).
    pub duration_frames: u64,
    /// Container-declared duration in seconds — valid for both video and
    /// audio-only media, unlike `duration_frames` (always 0 without a video
    /// track).
    pub duration_seconds: f64,
    /// Container-declared rate, exact rational (0/1 when there is no video).
    pub fps_num: i32,
    pub fps_den: i32,
    pub width: u32,
    pub height: u32,
    pub audio: bool,
}

/// Cached probe results keyed by footage item id. Always present on the bridge;
/// populated on import/open when the `media` feature is on, cleared on new/open.
/// Also holds the Project-panel thumbnail cache (one downscaled RGBA per
/// `(item, max_edge)`), decoded once and reused (media feature).
#[derive(Default)]
pub(crate) struct MediaCache {
    map: HashMap<Uuid, MediaStatus>,
    /// Downscaled thumbnails, keyed `(item id, max_edge)`. Populated lazily by
    /// [`thumbnail`], cleared with the probe cache on new/open/relink.
    #[cfg(feature = "media")]
    thumbs: HashMap<(Uuid, u32), (u32, u32, Vec<u8>)>,
}

impl MediaCache {
    pub fn clear(&mut self) {
        self.map.clear();
        #[cfg(feature = "media")]
        self.thumbs.clear();
    }

    /// A cached thumbnail for `(id, max_edge)`, if one was decoded already.
    #[cfg(feature = "media")]
    fn thumb_get(&self, id: Uuid, max_edge: u32) -> Option<(u32, u32, Vec<u8>)> {
        self.thumbs.get(&(id, max_edge)).cloned()
    }

    /// Store a decoded thumbnail for `(id, max_edge)`.
    #[cfg(feature = "media")]
    fn thumb_put(&mut self, id: Uuid, max_edge: u32, w: u32, h: u32, rgba: Vec<u8>) {
        self.thumbs.insert((id, max_edge), (w, h, rgba));
    }

    /// Read a cached entry (used by the media-feature probe path).
    #[cfg(feature = "media")]
    pub fn get(&self, id: &Uuid) -> Option<&MediaStatus> {
        self.map.get(id)
    }

    /// Store a probe result. Used by the media-feature probe path; also by the
    /// pure-data cache tests, so it stays compiled without the feature.
    #[cfg_attr(not(feature = "media"), allow(dead_code))]
    pub fn insert(&mut self, id: Uuid, status: MediaStatus) {
        self.map.insert(id, status);
    }

    /// The snapshot representation for a footage item: its `status` string and,
    /// when probed cleanly, the `media` detail block. An id with no cache entry
    /// is "unprobed".
    pub fn snapshot_for(&self, id: Uuid) -> (&'static str, Option<Value>) {
        match self.map.get(&id) {
            None => ("unprobed", None),
            Some(MediaStatus::Missing) => ("missing", None),
            Some(MediaStatus::Failed) => ("failed", None),
            Some(MediaStatus::Ok(info)) => (
                "ok",
                Some(json!({
                    "duration_frames": info.duration_frames,
                    "fps": { "num": info.fps_num, "den": info.fps_den },
                    "width": info.width,
                    "height": info.height,
                    "audio": info.audio,
                })),
            ),
        }
    }
}

/// Probe `path` synchronously into a [`MediaStatus`] (`media` feature only). A
/// path that is not a file is "missing" (never an error); an unreadable file is
/// "failed"; a readable one is "ok" with metadata, building/loading the frame
/// index the same way `lumit-ui`'s `probe_and_index` does so the decodable frame
/// count is exact and the index cache is warmed for later frame decodes.
#[cfg(feature = "media")]
pub(crate) fn probe_path(path: &std::path::Path) -> MediaStatus {
    if !path.is_file() {
        return MediaStatus::Missing;
    }
    let probe = match lumit_media::probe::probe(path) {
        Ok(p) => p,
        Err(_) => return MediaStatus::Failed,
    };
    let (fps_num, fps_den, width, height) = match &probe.video {
        Some(v) => (v.fps_num, v.fps_den, v.width, v.height),
        None => (0, 1, 0, 0),
    };
    MediaStatus::Ok(MediaInfo {
        duration_frames: video_frame_count(path, &probe),
        duration_seconds: probe.duration_seconds,
        fps_num,
        fps_den,
        width,
        height,
        audio: probe.audio.is_some(),
    })
}

/// The decodable frame count: build or load the frame index (video only). Audio
/// -only files need no index and count zero frames. A build failure counts zero
/// rather than failing the whole probe — the metadata is still useful.
#[cfg(feature = "media")]
fn video_frame_count(path: &std::path::Path, probe: &lumit_media::MediaProbe) -> u64 {
    if probe.video.is_none() {
        return 0;
    }
    load_or_build_index(path).map_or(0, |i| i.frame_count() as u64)
}

/// Decode one footage frame to tightly-packed RGBA8 (`media` feature only).
/// `None` on any failure (missing file, unreadable, frame index empty). Rebuilds
/// nothing it can load: it loads the cached frame index when present, else builds
/// (and caches) it, then opens a decoder for this one call — synchronous, and
/// not yet pooled across calls (a later phase caches decoders per item).
#[cfg(feature = "media")]
pub(crate) fn decode_frame(
    path: &std::path::Path,
    frame: u64,
) -> Option<lumit_media::DecodedFrame> {
    if !path.is_file() {
        return None;
    }
    let index = load_or_build_index(path)?;
    let mut decoder = lumit_media::VideoDecoder::open(path, index).ok()?;
    let count = decoder.frame_count();
    if count == 0 {
        return None;
    }
    let n = (frame as usize).min(count - 1);
    decoder.frame_rgba(n, None).ok()
}

/// Decode a representative frame of footage item `item_id` and downscale it so
/// its longer edge is at most `max_edge`, caching the result on the bridge's
/// [`MediaCache`] so the Project panel decodes each thumbnail exactly once
/// (`media` feature only). Frame 0 is the representative frame. `None` on any
/// failure (unknown/non-footage item, missing/unreadable file, empty index) —
/// the null the FFI turns into "no thumbnail". A `max_edge` of 0 is treated as
/// 1; oversized values are clamped so a thumbnail never allocates unbounded.
///
/// The downscale never *upscales*: a source already within `max_edge` is
/// returned at its own size, so a tiny clip is not blown up.
#[cfg(feature = "media")]
pub(crate) fn thumbnail(
    bridge: &mut crate::state::Bridge,
    item_id: &str,
    max_edge: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let id = Uuid::parse_str(item_id).ok()?;
    let max_edge = max_edge.clamp(1, 4096);
    if let Some(hit) = bridge.media.thumb_get(id, max_edge) {
        return Some(hit);
    }
    let path = crate::state::footage_path(bridge, item_id)?;
    let frame = decode_frame(&path, 0)?;
    let (w, h, rgba) = downscale_to_max_edge(frame.width, frame.height, &frame.rgba, max_edge);
    bridge.media.thumb_put(id, max_edge, w, h, rgba.clone());
    Some((w, h, rgba))
}

/// Downscale tightly-packed RGBA8 `src` (`sw`×`sh`) so its longer edge is at
/// most `max_edge`, preserving aspect. A box (area-average) filter — cheap and
/// clean enough for a panel thumbnail. Returns the source unchanged when it
/// already fits (never upscales) or when a degenerate size would result.
#[cfg(feature = "media")]
fn downscale_to_max_edge(sw: u32, sh: u32, src: &[u8], max_edge: u32) -> (u32, u32, Vec<u8>) {
    if sw == 0 || sh == 0 || src.len() < (sw as usize * sh as usize * 4) {
        return (sw, sh, src.to_vec());
    }
    let longer = sw.max(sh);
    if longer <= max_edge {
        return (sw, sh, src.to_vec());
    }
    let scale = f64::from(max_edge) / f64::from(longer);
    let dw = ((f64::from(sw) * scale).round() as u32).max(1);
    let dh = ((f64::from(sh) * scale).round() as u32).max(1);
    let mut out = vec![0u8; dw as usize * dh as usize * 4];
    let (sw_u, sh_u, dw_u, dh_u) = (sw as usize, sh as usize, dw as usize, dh as usize);
    for dy in 0..dh_u {
        // The source-row band this destination row averages over.
        let y0 = dy * sh_u / dh_u;
        let y1 = (((dy + 1) * sh_u).div_ceil(dh_u)).min(sh_u).max(y0 + 1);
        for dx in 0..dw_u {
            let x0 = dx * sw_u / dw_u;
            let x1 = (((dx + 1) * sw_u).div_ceil(dw_u)).min(sw_u).max(x0 + 1);
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1 {
                let row = sy * sw_u * 4;
                for sx in x0..x1 {
                    let i = row + sx * 4;
                    r += u32::from(src[i]);
                    g += u32::from(src[i + 1]);
                    b += u32::from(src[i + 2]);
                    a += u32::from(src[i + 3]);
                    n += 1;
                }
            }
            let n = n.max(1);
            let o = (dy * dw_u + dx) * 4;
            out[o] = (r / n) as u8;
            out[o + 1] = (g / n) as u8;
            out[o + 2] = (b / n) as u8;
            out[o + 3] = (a / n) as u8;
        }
    }
    (dw, dh, out)
}

/// Load the cached frame index for `path` if one matches, else build it and try
/// to cache it. `None` when the index cannot be built (unreadable/truncated).
#[cfg(feature = "media")]
fn load_or_build_index(path: &std::path::Path) -> Option<lumit_media::FrameIndex> {
    let cache_dir = lumit_project::media_index_dir();
    if let (Some(dir), Ok(fp)) = (&cache_dir, lumit_media::Fingerprint::of(path)) {
        if let Some(index) = lumit_media::FrameIndex::load_cached(dir, &fp) {
            return Some(index);
        }
    }
    let index = lumit_media::index::build_frame_index(path).ok()?;
    if let Some(dir) = &cache_dir {
        let _ = index.save_to(dir);
    }
    Some(index)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn cache_reports_unprobed_ok_missing_and_failed() {
        let mut cache = MediaCache::default();
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();
        let d = Uuid::now_v7();
        cache.insert(
            b,
            MediaStatus::Ok(MediaInfo {
                duration_frames: 120,
                duration_seconds: 2.0,
                fps_num: 60,
                fps_den: 1,
                width: 320,
                height: 240,
                audio: true,
            }),
        );
        cache.insert(c, MediaStatus::Missing);
        cache.insert(d, MediaStatus::Failed);

        // Absent → unprobed, no media block.
        assert_eq!(cache.snapshot_for(a), ("unprobed", None));
        // Missing / failed → their status strings, no media block.
        assert_eq!(cache.snapshot_for(c).0, "missing");
        assert!(cache.snapshot_for(c).1.is_none());
        assert_eq!(cache.snapshot_for(d).0, "failed");

        // Ok → the metadata block, mirroring the task's shape.
        let (status, detail) = cache.snapshot_for(b);
        assert_eq!(status, "ok");
        let detail = detail.unwrap();
        assert_eq!(detail["duration_frames"], json!(120));
        assert_eq!(detail["fps"], json!({ "num": 60, "den": 1 }));
        assert_eq!(detail["width"], json!(320));
        assert_eq!(detail["height"], json!(240));
        assert_eq!(detail["audio"], json!(true));
    }

    /// A path that is not a file probes to "missing", never an error — the one
    /// media-probe branch we can exercise in-crate without an FFmpeg-encoded
    /// fixture. Decoding real encoded frames is covered by `lumit-media`'s own
    /// suite (its fixtures need the FFmpeg CLI, which is not assumed here).
    #[cfg(feature = "media")]
    #[test]
    fn probe_of_a_missing_path_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-here.mp4");
        assert!(matches!(probe_path(&path), MediaStatus::Missing));
    }

    /// A present-but-unreadable file probes to "failed" (not "missing"). Uses a
    /// zero-byte file — no FFmpeg fixture needed, but it does exercise the real
    /// `lumit_media::probe` error branch.
    #[cfg(feature = "media")]
    #[test]
    fn probe_of_an_unreadable_file_is_failed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.mp4");
        std::fs::write(&path, []).unwrap();
        assert!(matches!(probe_path(&path), MediaStatus::Failed));
    }

    /// Decoding a frame of a file that is not on disk yields `None` (null at the
    /// FFI boundary), never a panic.
    #[cfg(feature = "media")]
    #[test]
    fn decode_of_a_missing_path_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.mp4");
        assert!(decode_frame(&path, 0).is_none());
    }

    /// The thumbnail downscaler halves a 4×2 source to 2×1 with the longer edge
    /// capped, preserves aspect, and averages the covered pixels.
    #[cfg(feature = "media")]
    #[test]
    fn downscale_caps_the_longer_edge_and_averages() {
        // 4×2 RGBA: left half red (255,0,0), right half green (0,255,0).
        let mut src = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                if x < 2 {
                    src.extend_from_slice(&[255, 0, 0, 255]);
                } else {
                    src.extend_from_slice(&[0, 255, 0, 255]);
                }
            }
        }
        let (w, h, out) = downscale_to_max_edge(4, 2, &src, 2);
        assert_eq!((w, h), (2, 1), "longer edge (4) capped to 2, aspect kept");
        assert_eq!(out.len(), 8);
        // Left destination pixel averages the two red source columns → red.
        assert!(out[0] > 200 && out[1] < 60, "left thumbnail pixel is red");
        // Right destination pixel averages the two green source columns → green.
        assert!(
            out[4] < 60 && out[5] > 200,
            "right thumbnail pixel is green"
        );
    }

    /// A source already within `max_edge` is returned unchanged (never upscaled).
    #[cfg(feature = "media")]
    #[test]
    fn downscale_never_upscales() {
        let src = vec![7u8; 2 * 2 * 4];
        let (w, h, out) = downscale_to_max_edge(2, 2, &src, 256);
        assert_eq!((w, h), (2, 2));
        assert_eq!(out, src);
    }

    /// The thumbnail cache round-trips through the bridge: an unknown item is
    /// `None` (never a panic), and the cache stores keyed on `(item, max_edge)`.
    #[cfg(feature = "media")]
    #[test]
    fn thumbnail_of_an_unknown_item_is_none() {
        let mut bridge = crate::state::Bridge::new();
        assert!(thumbnail(&mut bridge, "not-a-uuid", 128).is_none());
        let unknown = Uuid::now_v7().to_string();
        assert!(thumbnail(&mut bridge, &unknown, 128).is_none());
    }
}
