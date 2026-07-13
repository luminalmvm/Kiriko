//! Media probing and the frame index — docs/impl/media-io.md §2, slice 4.
//!
//! In plain terms: when footage is imported, Kiriko reads the file's vital
//! statistics (resolution, frame rate, duration — the *probe*) and then scans
//! every packet without decoding to build the *frame index*: an exact map of
//! frame number → timestamp → nearest keyframe. The index is what makes
//! scrubbing land on exactly the right frame in slice 5, and it is cached on
//! disk keyed by a content *fingerprint* so it is built once per file.

pub mod audio;
pub mod decode;
pub mod index;
pub mod probe;

use std::path::Path;

pub use audio::AudioBuffer;
pub use decode::{DecodedFrame, VideoDecoder};
pub use index::{FrameIndex, IndexEntry};
pub use probe::{AudioInfo, MediaProbe, VideoInfo};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("ffmpeg: {0}")]
    Ffmpeg(String),
    #[error("path is not valid unicode")]
    BadPath,
    #[error("no streams found")]
    NoStreams,
    #[error("index cache: {0}")]
    IndexCache(String),
}

impl From<rsmpeg::error::RsmpegError> for MediaError {
    fn from(e: rsmpeg::error::RsmpegError) -> Self {
        MediaError::Ffmpeg(e.to_string())
    }
}

/// The linked FFmpeg (libavformat) version, for the boot log (K-008).
pub fn ffmpeg_version() -> String {
    format!(
        "{}.{}.{}",
        rsmpeg::ffi::LIBAVFORMAT_VERSION_MAJOR,
        rsmpeg::ffi::LIBAVFORMAT_VERSION_MINOR,
        rsmpeg::ffi::LIBAVFORMAT_VERSION_MICRO
    )
}

/// Content fingerprint for relinking and index-cache keys
/// (docs/03-DATA-MODEL.md §3): size + mtime + hash of head and tail.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fingerprint {
    pub size: u64,
    pub mtime_unix: i64,
    pub content_hash: String, // blake3 of first + last 64 KiB, hex
}

impl Fingerprint {
    pub fn of(path: &Path) -> Result<Self, MediaError> {
        use std::io::{Read, Seek, SeekFrom};
        let meta = std::fs::metadata(path)?;
        let size = meta.len();
        let mtime_unix = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);

        let mut file = std::fs::File::open(path)?;
        let mut hasher = blake3::Hasher::new();
        let chunk = 64 * 1024;
        let mut buf = vec![0u8; chunk];
        let read = file.read(&mut buf)?;
        hasher.update(&buf[..read]);
        if size > (2 * chunk) as u64 {
            file.seek(SeekFrom::End(-(chunk as i64)))?;
            let read = file.read(&mut buf)?;
            hasher.update(&buf[..read]);
        }
        Ok(Self {
            size,
            mtime_unix,
            content_hash: hasher.finalize().to_hex().to_string(),
        })
    }

    /// Stable key for cache filenames.
    pub fn cache_key(&self) -> String {
        format!("{}-{}", &self.content_hash[..32], self.size)
    }
}
