//! The preview's wire format (ADR-0014): each binary message is a header,
//! then its payload. Numbers are little-endian.
//!
//! | Offset | Bytes | Field |
//! |---|---|---|
//! | 0 | 4 | `OSPV` |
//! | 4 | 1 | the version: 1 |
//! | 5 | 1 | the kind: 1 for a frame |
//! | 6 | 1 | the codec: 1 for JPEG |
//! | 7 | 1 | the JPEG quality |
//! | 8 | 4 | the header's length: where the payload starts |
//! | 12 | 4 | the message's number in its session, from 0 |
//! | 16 | 8 | the frame's ID |
//! | 24 | 8 | when its exposure started, in ns on the server's clock |
//! | 32 | 8 | when its last row's exposure ended |
//! | 40 | 8 | when the message was ready |
//! | 48 | 16 | the sensor's pixels it shows: x, y, width and height |
//! | 64 | 4 | the image's width in pixels |
//! | 68 | 4 | its height |
//!
//! Fields are only ever added at the end, so a reader takes the payload
//! from the header's length. The version changes only if a field changes
//! its meaning, and a reader refuses a version it doesn't know.

use oscope_core::{FrameId, PixelRect, Timestamp};

/// The first four bytes of every message.
pub const MAGIC: [u8; 4] = *b"OSPV";
/// The version this crate writes and reads.
pub const VERSION: u8 = 1;
/// The kind of a frame's message.
pub const FRAME: u8 = 1;
/// The codec of a JPEG payload.
pub const JPEG: u8 = 1;

/// A message's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// What the message is: [`FRAME`].
    pub kind: u8,
    /// How its payload is coded: [`JPEG`].
    pub codec: u8,
    /// The JPEG quality.
    pub quality: u8,
    /// The message's number in its session.
    pub sequence: u32,
    /// The frame it shows.
    pub frame: FrameId,
    /// When the frame's exposure started.
    pub exposure_start: Timestamp,
    /// When its last row's exposure ended.
    pub exposure_end: Timestamp,
    /// When the message was ready to send.
    pub ready: Timestamp,
    /// The sensor's pixels it shows, in full-sensor coordinates.
    pub roi: PixelRect,
    /// The image's width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// What can't be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WireError {
    /// Fewer bytes than a header.
    #[error("{0} bytes, shorter than a header")]
    Short(usize),
    /// It doesn't start with `OSPV`.
    #[error("not a preview message")]
    Magic,
    /// A version this crate doesn't know.
    #[error("version {0}, where {VERSION} is known")]
    Version(u8),
}

impl Header {
    /// The length this version writes.
    pub const LEN: usize = 72;

    /// The message: this header, then `payload`.
    pub fn message(&self, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::LEN + payload.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&[VERSION, self.kind, self.codec, self.quality]);
        out.extend_from_slice(&(Self::LEN as u32).to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        for time in [
            self.frame.0,
            self.exposure_start.0,
            self.exposure_end.0,
            self.ready.0,
        ] {
            out.extend_from_slice(&time.to_le_bytes());
        }
        let r = self.roi;
        for n in [r.x, r.y, r.width, r.height, self.width, self.height] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        out.extend_from_slice(payload);
        out
    }

    /// A message's header and payload.
    pub fn read(message: &[u8]) -> Result<(Header, &[u8]), WireError> {
        if message.len() < Self::LEN {
            return Err(WireError::Short(message.len()));
        }
        if message[..4] != MAGIC {
            return Err(WireError::Magic);
        }
        if message[4] != VERSION {
            return Err(WireError::Version(message[4]));
        }
        let u32_at = |at: usize| u32::from_le_bytes(message[at..at + 4].try_into().unwrap());
        let u64_at = |at: usize| u64::from_le_bytes(message[at..at + 8].try_into().unwrap());
        let length = u32_at(8) as usize;
        if length < Self::LEN || length > message.len() {
            return Err(WireError::Short(message.len()));
        }
        let header = Header {
            kind: message[5],
            codec: message[6],
            quality: message[7],
            sequence: u32_at(12),
            frame: FrameId(u64_at(16)),
            exposure_start: Timestamp(u64_at(24)),
            exposure_end: Timestamp(u64_at(32)),
            ready: Timestamp(u64_at(40)),
            roi: PixelRect::new(u32_at(48), u32_at(52), u32_at(56), u32_at(60)),
            width: u32_at(64),
            height: u32_at(68),
        };
        Ok((header, &message[length..]))
    }
}
