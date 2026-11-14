//! A sample importer and exporter (P30, ADR-0037): PGM, Netpbm's grey
//! images, binary (`P5`), of 8 or 16 bits. A stack is its images one after
//! another.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::{Mutex, MutexGuard, PoisonError};

use oscope_ext_sdk::{Exporter, Format, ImageDescription, Importer, Sample};

fn format() -> Format {
    Format {
        name: "PGM (Netpbm)".to_owned(),
        extensions: vec!["pgm".to_owned()],
    }
}

/// A PGM image's width, height, largest sample and where its samples start,
/// in `bytes` from `at`.
fn header(bytes: &[u8], mut at: usize) -> Result<(u64, u64, u32, usize), String> {
    if bytes.get(at..at + 2) != Some(b"P5") {
        return Err("not a binary PGM (P5)".to_owned());
    }
    at += 2;
    let mut fields = [0_u64; 3];
    for field in &mut fields {
        // Whitespace, and comments to the end of their lines.
        while let Some(&b) = bytes.get(at) {
            match b {
                b'#' => {
                    at += bytes[at..]
                        .iter()
                        .position(|&b| b == b'\n')
                        .unwrap_or(bytes.len() - at)
                }
                b if b.is_ascii_whitespace() => at += 1,
                _ => break,
            }
        }
        let digits = bytes[at..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        let text = std::str::from_utf8(&bytes[at..at + digits]).map_err(|e| e.to_string())?;
        *field = text
            .parse()
            .map_err(|_| "a header that isn't three numbers".to_owned())?;
        at += digits;
    }
    let [width, height, most] = fields;
    match most {
        1..=65_535 => Ok((width, height, most as u32, at + 1)),
        _ => Err(format!("a largest sample of {most}")),
    }
}

/// Each image's start in `bytes`, after the first's header.
fn images(bytes: &[u8]) -> Result<(ImageDescription, Vec<usize>), String> {
    let (width, height, most, first) = header(bytes, 0)?;
    let size = (width * height) as usize * if most < 256 { 1 } else { 2 };
    let mut starts = vec![first];
    while starts.last().is_some_and(|s| s + size < bytes.len()) {
        let next = header(bytes, starts[starts.len() - 1] + size)?;
        if (next.0, next.1, next.2) != (width, height, most) {
            return Err("its images differ in size or depth".to_owned());
        }
        starts.push(next.3);
    }
    let image = ImageDescription {
        width,
        height,
        channels: 1,
        planes: starts.len() as u32,
        sample: if most < 256 { Sample::U8 } else { Sample::U16 },
        pixel_size: None,
        channel_names: vec!["grey".to_owned()],
    };
    Ok((image, starts))
}

/// Reads PGM files: `example.pgm.read`.
pub struct PgmReader;

impl Importer for PgmReader {
    fn format(&self) -> Format {
        format()
    }

    fn probe(&self, path: &str) -> Result<ImageDescription, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        Ok(images(&bytes)?.0)
    }

    fn read(&self, path: &str, plane: u32, pixels: &mut [u8]) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let (image, starts) = images(&bytes)?;
        let start = *starts.get(plane as usize).ok_or("no such plane")?;
        let raster = bytes
            .get(start..start + pixels.len())
            .ok_or("a file cut short")?;
        match image.sample {
            // PGM's 16 bits are big-endian; OScope's little-endian.
            Sample::U16 => {
                let samples = raster.as_chunks::<2>().0;
                for (out, sample) in pixels.as_chunks_mut::<2>().0.iter_mut().zip(samples) {
                    *out = [sample[1], sample[0]];
                }
            }
            _ => pixels.copy_from_slice(raster),
        }
        Ok(())
    }
}

/// Writes PGM files: `example.pgm.write`, of one grey channel of 8 or 16
/// bits.
#[derive(Default)]
pub struct PgmWriter {
    open: Mutex<Option<(BufWriter<File>, ImageDescription)>>,
}

impl PgmWriter {
    fn open(&self) -> MutexGuard<'_, Option<(BufWriter<File>, ImageDescription)>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Exporter for PgmWriter {
    fn format(&self) -> Format {
        format()
    }

    fn begin(&self, path: &str, image: &ImageDescription, _name: &str) -> Result<(), String> {
        if image.channels != 1 || image.sample == Sample::F32 {
            return Err("a PGM is one grey channel of 8 or 16 bits".to_owned());
        }
        let file = File::create(path).map_err(|e| e.to_string())?;
        *self.open() = Some((BufWriter::new(file), image.clone()));
        Ok(())
    }

    fn write(&self, _plane: u32, pixels: &[u8]) -> Result<(), String> {
        let mut open = self.open();
        let (out, image) = open.as_mut().ok_or("nothing begun")?;
        let most = if image.sample == Sample::U8 {
            255
        } else {
            65_535
        };
        let header = format!("P5\n{} {}\n{most}\n", image.width, image.height);
        out.write_all(header.as_bytes())
            .map_err(|e| e.to_string())?;
        let raster: Vec<u8> = match image.sample {
            Sample::U16 => pixels
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|s| [s[1], s[0]])
                .collect(),
            _ => pixels.to_vec(),
        };
        out.write_all(&raster).map_err(|e| e.to_string())
    }

    fn end(&self) -> Result<(), String> {
        let (mut out, _) = self.open().take().ok_or("nothing begun")?;
        out.flush().map_err(|e| e.to_string())
    }
}
