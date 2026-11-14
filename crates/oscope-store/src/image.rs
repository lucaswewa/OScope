//! Images as OME-Zarr v3 stores (D15, ADR-0021), through `zarrs`: a Zarr
//! group holding a multiscale pyramid of arrays, `c, z, y, x`, with NGFF
//! 0.5's metadata, so that napari and Python's OME-Zarr readers open them.
//!
//! - **Chunks** hold every channel of 512 × 512 pixels of one plane: a
//!   deep-zoom tile of 512, or two by two of 256 (P40); and a stack is read a
//!   plane at a time.
//! - **zstd** compresses each chunk.
//! - **Level 0** is written in regions, as the caller has them;
//!   [`ImageWriter::finish`] then makes each level from the one before, the
//!   mean of each 2 × 2 pixels, until a level fits one chunk.

use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use zarrs::array::codec::ZstdCodec;
use zarrs::array::{Array, ArrayBuilder, DataType, Element, ElementOwned};
use zarrs::array_subset::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::group::{Group, GroupBuilder};

use crate::{Result, StoreError};

/// The side of a chunk, in pixels.
pub const CHUNK: u64 = 512;

/// zstd's level: fast to write, and within a few per cent of its smallest.
const ZSTD_LEVEL: i32 = 3;

fn zarr(error: impl std::fmt::Display) -> StoreError {
    StoreError::Zarr(error.to_string())
}

/// The type of an image's samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SampleType {
    /// 8 bits, unsigned.
    U8,
    /// 16 bits, unsigned.
    U16,
    /// 32-bit floating point.
    F32,
}

impl SampleType {
    fn data_type(self) -> DataType {
        match self {
            Self::U8 => DataType::UInt8,
            Self::U16 => DataType::UInt16,
            Self::F32 => DataType::Float32,
        }
    }

    /// The largest value a display shows as white.
    fn white(self) -> f64 {
        match self {
            Self::U8 => 255.0,
            Self::U16 => 65_535.0,
            Self::F32 => 1.0,
        }
    }
}

/// Samples of a region of an image, interleaved: row by row, each pixel's
/// channels together.
#[derive(Clone, Debug, PartialEq)]
pub enum Samples {
    /// 8-bit samples.
    U8(Vec<u8>),
    /// 16-bit samples.
    U16(Vec<u16>),
    /// Floating-point samples.
    F32(Vec<f32>),
}

impl Samples {
    /// How many samples there are.
    pub fn len(&self) -> usize {
        match self {
            Self::U8(v) => v.len(),
            Self::U16(v) => v.len(),
            Self::F32(v) => v.len(),
        }
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Their type.
    pub fn sample_type(&self) -> SampleType {
        match self {
            Self::U8(_) => SampleType::U8,
            Self::U16(_) => SampleType::U16,
            Self::F32(_) => SampleType::F32,
        }
    }

    /// Each sample from 0 to 1: 8 and 16-bit samples over their range, and
    /// floating-point ones as they are.
    pub fn to_unit(&self) -> Vec<f32> {
        match self {
            Self::U8(v) => v.iter().map(|&s| f32::from(s) / 255.0).collect(),
            Self::U16(v) => v.iter().map(|&s| f32::from(s) / 65_535.0).collect(),
            Self::F32(v) => v.clone(),
        }
    }
}

/// The physical size of a pixel, and the step between planes, in µm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PixelSize {
    /// Across, in µm.
    pub x: f64,
    /// Down, in µm.
    pub y: f64,
    /// Between planes, in µm.
    pub z: f64,
}

/// An image: its shape, its samples and its scale.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImageInfo {
    /// Its width in pixels, at full resolution.
    pub width: u64,
    /// Its height in pixels, at full resolution.
    pub height: u64,
    /// Its channels: 1 for grey, 3 for RGB.
    pub channels: u32,
    /// Its planes: 1, or a stack's.
    pub planes: u32,
    /// The type of its samples.
    pub sample: SampleType,
    /// The size of its pixels, if known.
    #[serde(default)]
    pub pixel_size: Option<PixelSize>,
    /// Its channels' names: `channel 0`, `channel 1`… if none are given.
    #[serde(default)]
    pub channel_names: Vec<String>,
    /// How many levels its pyramid has, full resolution among them. Set by
    /// the writer.
    #[serde(default)]
    pub levels: u32,
}

impl ImageInfo {
    /// An image of `width` × `height` pixels, of `channels` channels of
    /// `sample`s, in one plane.
    pub fn new(width: u64, height: u64, channels: u32, sample: SampleType) -> Self {
        Self {
            width,
            height,
            channels,
            planes: 1,
            sample,
            pixel_size: None,
            channel_names: Vec::new(),
            levels: levels(width, height),
        }
    }

    /// The width and height of level `level`.
    pub fn level_size(&self, level: u32) -> (u64, u64) {
        (0..level).fold((self.width, self.height), |(w, h), _| {
            (w.div_ceil(2), h.div_ceil(2))
        })
    }
}

/// How many levels an image of `width` × `height` has: halving until a level
/// fits one chunk.
fn levels(width: u64, height: u64) -> u32 {
    let (mut w, mut h, mut levels) = (width, height, 1);
    while w > CHUNK || h > CHUNK {
        (w, h, levels) = (w.div_ceil(2), h.div_ceil(2), levels + 1);
    }
    levels
}

/// The group's attributes: NGFF 0.5's `ome` key.
fn attributes(info: &ImageInfo, name: &str) -> Map<String, Value> {
    let size = info.pixel_size.unwrap_or(PixelSize {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    });
    let space = |axis: &str| match info.pixel_size {
        Some(_) => json!({"name": axis, "type": "space", "unit": "micrometer"}),
        None => json!({"name": axis, "type": "space"}),
    };
    let datasets: Vec<Value> = (0..info.levels)
        .map(|level| {
            let f = f64::from(1_u32 << level);
            let mut transforms = vec![json!({"type": "scale", "scale": [1.0, size.z, size.y * f, size.x * f]})];
            // A pixel of level k is the mean of 2^k × 2^k of level 0: its
            // centre is (2^k − 1) / 2 of level 0's pixels further on.
            if level > 0 {
                let shift = |s: f64| s * (f - 1.0) / 2.0;
                transforms.push(json!({"type": "translation", "translation": [0.0, 0.0, shift(size.y), shift(size.x)]}));
            }
            json!({"path": level.to_string(), "coordinateTransformations": transforms})
        })
        .collect();
    let colours: &[&str] = match info.channels {
        3 => &["FF0000", "00FF00", "0000FF"],
        _ => &["FFFFFF"],
    };
    let white = info.sample.white();
    let channels: Vec<Value> = (0..info.channels as usize)
        .map(|c| {
            let label = info.channel_names[c].clone();
            let window = json!({"min": 0.0, "max": white, "start": 0.0, "end": white});
            json!({"label": label, "color": colours[c % colours.len()], "window": window, "active": true})
        })
        .collect();
    let multiscale = json!({
        "name": name,
        "axes": [{"name": "c", "type": "channel"}, space("z"), space("y"), space("x")],
        "datasets": datasets,
        "type": "mean",
        "metadata": {
            "description": "each level the mean of 2 × 2 pixels of the one before",
            "method": "oscope_store::ImageWriter",
            "version": env!("CARGO_PKG_VERSION"),
        },
    });
    let ome =
        json!({"version": "0.5", "multiscales": [multiscale], "omero": {"channels": channels}});
    Map::from_iter([("ome".to_owned(), ome)])
}

/// A sample type `zarrs` stores, averaged when the pyramid is made.
trait Sample: Element + ElementOwned + Copy + Default + Send + Sync + 'static {
    fn to_f64(self) -> f64;
    fn from_mean(mean: f64) -> Self;
}

impl Sample for u8 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_mean(mean: f64) -> Self {
        mean.round().clamp(0.0, 255.0) as u8
    }
}

impl Sample for u16 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_mean(mean: f64) -> Self {
        mean.round().clamp(0.0, 65_535.0) as u16
    }
}

impl Sample for f32 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_mean(mean: f64) -> Self {
        mean as f32
    }
}

/// A region of one plane: where, how large, and of how many channels.
#[derive(Clone, Copy)]
struct Region {
    plane: u64,
    x: u64,
    y: u64,
    width: u64,
    height: u64,
    channels: u64,
}

impl Region {
    fn subset(&self) -> ArraySubset {
        ArraySubset::new_with_ranges(&[
            0..self.channels,
            self.plane..self.plane + 1,
            self.y..self.y + self.height,
            self.x..self.x + self.width,
        ])
    }

    fn samples(&self) -> usize {
        (self.width * self.height * self.channels) as usize
    }
}

/// Stores interleaved `samples` in `region` of `array`, whose order is
/// channel by channel.
fn store<T: Sample>(array: &Array<FilesystemStore>, region: Region, samples: &[T]) -> Result<()> {
    let (pixels, channels) = (
        (region.width * region.height) as usize,
        region.channels as usize,
    );
    let mut planar = vec![T::default(); samples.len()];
    for (i, pixel) in samples.chunks_exact(channels).enumerate() {
        for (c, sample) in pixel.iter().enumerate() {
            planar[c * pixels + i] = *sample;
        }
    }
    array
        .store_array_subset_elements(&region.subset(), &planar)
        .map_err(zarr)
}

/// The samples of `region` of `array`, interleaved.
fn retrieve<T: Sample>(array: &Array<FilesystemStore>, region: Region) -> Result<Vec<T>> {
    let planar: Vec<T> = array
        .retrieve_array_subset_elements(&region.subset())
        .map_err(zarr)?;
    let (pixels, channels) = (
        (region.width * region.height) as usize,
        region.channels as usize,
    );
    let mut samples = vec![T::default(); planar.len()];
    for (c, plane) in planar.chunks_exact(pixels.max(1)).enumerate() {
        for (i, sample) in plane.iter().enumerate() {
            samples[i * channels + c] = *sample;
        }
    }
    Ok(samples)
}

/// The mean of each 2 × 2 pixels of interleaved `samples`, `width` ×
/// `height` × `channels`; at an odd edge, of the pixels there are.
fn halve<T: Sample>(samples: &[T], width: usize, height: usize, channels: usize) -> Vec<T> {
    let (w, h) = (width.div_ceil(2), height.div_ceil(2));
    let mut out = Vec::with_capacity(w * h * channels);
    for y in 0..h {
        for x in 0..w {
            let rows = (2 * y)..(2 * y + 2).min(height);
            let cols = (2 * x)..(2 * x + 2).min(width);
            let count = (rows.len() * cols.len()) as f64;
            for c in 0..channels {
                let mut sum = 0.0;
                for yy in rows.clone() {
                    for xx in cols.clone() {
                        sum += samples[(yy * width + xx) * channels + c].to_f64();
                    }
                }
                out.push(T::from_mean(sum / count));
            }
        }
    }
    out
}

/// Makes level `level` of plane `plane` from the level before, a chunk at a
/// time.
fn make_level<T: Sample>(
    arrays: &[Array<FilesystemStore>],
    info: &ImageInfo,
    level: u32,
    plane: u64,
) -> Result<()> {
    let (source, target) = (&arrays[level as usize - 1], &arrays[level as usize]);
    let (sw, sh) = info.level_size(level - 1);
    let (tw, th) = info.level_size(level);
    let channels = u64::from(info.channels);
    for y in (0..th).step_by(CHUNK as usize) {
        for x in (0..tw).step_by(CHUNK as usize) {
            let (width, height) = (CHUNK.min(tw - x), CHUNK.min(th - y));
            let from = Region {
                plane,
                x: 2 * x,
                y: 2 * y,
                width: (2 * width).min(sw - 2 * x),
                height: (2 * height).min(sh - 2 * y),
                channels,
            };
            let samples: Vec<T> = retrieve(source, from)?;
            let halved = halve(
                &samples,
                from.width as usize,
                from.height as usize,
                channels as usize,
            );
            let to = Region {
                plane,
                x,
                y,
                width,
                height,
                channels,
            };
            store(target, to, &halved)?;
        }
    }
    Ok(())
}

/// Checks that `info` describes an image the store can hold.
fn check(info: &ImageInfo) -> Result<()> {
    let refused = |why: &str| Err(StoreError::Refused(format!("an image {why}")));
    if info.width == 0 || info.height == 0 || info.planes == 0 {
        return refused("needs at least one pixel and one plane");
    }
    if !(1..=4).contains(&info.channels) {
        return refused("has 1 to 4 channels");
    }
    if !info.channel_names.is_empty() && info.channel_names.len() != info.channels as usize {
        return refused("names each of its channels, or none");
    }
    match info.pixel_size {
        Some(PixelSize { x, y, z })
            if !(x > 0.0 && y > 0.0 && z > 0.0 && (x + y + z).is_finite()) =>
        {
            refused("has pixels of a positive, finite size")
        }
        _ => Ok(()),
    }
}

/// An image being written: an OME-Zarr store whose pyramid
/// [`finish`](Self::finish) makes.
pub struct ImageWriter {
    info: ImageInfo,
    arrays: Vec<Array<FilesystemStore>>,
}

impl ImageWriter {
    /// Makes the store at `path`, a folder, for an image as `info`
    /// describes, named `name` in its metadata. Its samples are the fill
    /// value, 0, until written.
    pub fn create(path: &Path, info: &ImageInfo, name: &str) -> Result<Self> {
        check(info)?;
        let mut info = info.clone();
        info.levels = levels(info.width, info.height);
        if info.channel_names.is_empty() {
            info.channel_names = (0..info.channels).map(|c| format!("channel {c}")).collect();
        }
        std::fs::create_dir_all(path).map_err(crate::io(path))?;
        let storage = Arc::new(FilesystemStore::new(path).map_err(zarr)?);
        let group = GroupBuilder::new()
            .attributes(attributes(&info, name))
            .build(Arc::clone(&storage), "/")
            .map_err(zarr)?;
        group.store_metadata().map_err(zarr)?;
        let channels = u64::from(info.channels);
        let arrays = (0..info.levels)
            .map(|level| {
                let (width, height) = info.level_size(level);
                let shape = vec![channels, u64::from(info.planes), height, width];
                let chunk = vec![channels, 1, CHUNK, CHUNK];
                let mut builder = match info.sample {
                    SampleType::U8 => {
                        ArrayBuilder::new(shape, chunk, info.sample.data_type(), 0_u8)
                    }
                    SampleType::U16 => {
                        ArrayBuilder::new(shape, chunk, info.sample.data_type(), 0_u16)
                    }
                    SampleType::F32 => {
                        ArrayBuilder::new(shape, chunk, info.sample.data_type(), 0.0_f32)
                    }
                };
                let array = builder
                    .bytes_to_bytes_codecs(vec![Arc::new(ZstdCodec::new(ZSTD_LEVEL, false))])
                    .dimension_names(Some(["c", "z", "y", "x"]))
                    .build(Arc::clone(&storage), &format!("/{level}"))
                    .map_err(zarr)?;
                array.store_metadata().map_err(zarr)?;
                Ok(array)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { info, arrays })
    }

    /// The image, as it will be.
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// Writes `samples`, interleaved, as the pixels of plane `plane` whose
    /// top-left is (`x`, `y`), `width` × `height` of them, at full
    /// resolution. A chunk is read, changed and written back, so regions
    /// that share a chunk must not be written at once.
    pub fn write(
        &self,
        plane: u32,
        (x, y): (u64, u64),
        (width, height): (u64, u64),
        samples: &Samples,
    ) -> Result<()> {
        let info = &self.info;
        let region = Region {
            plane: u64::from(plane),
            x,
            y,
            width,
            height,
            channels: u64::from(info.channels),
        };
        let inside = plane < info.planes && x + width <= info.width && y + height <= info.height;
        if !inside || width == 0 || height == 0 {
            return Err(StoreError::Refused(format!(
                "a region of {width} × {height} at ({x}, {y}) in plane {plane}: the image is {} × {} in {} planes",
                info.width, info.height, info.planes
            )));
        }
        if samples.sample_type() != info.sample || samples.len() != region.samples() {
            return Err(StoreError::Refused(format!(
                "{} samples of {:?} for {} of {:?}",
                samples.len(),
                samples.sample_type(),
                region.samples(),
                info.sample
            )));
        }
        let level0 = &self.arrays[0];
        match samples {
            Samples::U8(v) => store(level0, region, v),
            Samples::U16(v) => store(level0, region, v),
            Samples::F32(v) => store(level0, region, v),
        }
    }

    /// Makes the pyramid's other levels from what was written: the image.
    pub fn finish(self) -> Result<ImageInfo> {
        let (arrays, info) = (&self.arrays, &self.info);
        for level in 1..info.levels {
            (0..u64::from(info.planes))
                .into_par_iter()
                .try_for_each(|plane| match info.sample {
                    SampleType::U8 => make_level::<u8>(arrays, info, level, plane),
                    SampleType::U16 => make_level::<u16>(arrays, info, level, plane),
                    SampleType::F32 => make_level::<f32>(arrays, info, level, plane),
                })?;
        }
        Ok(self.info)
    }
}

/// An OME-Zarr store, open to read: one this crate wrote, or another with
/// one multiscale image of axes `c, z, y, x`.
pub struct ImageReader {
    info: ImageInfo,
    arrays: Vec<Array<FilesystemStore>>,
}

impl ImageReader {
    /// Opens the store at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let storage = Arc::new(FilesystemStore::new(path).map_err(zarr)?);
        let group = Group::open(Arc::clone(&storage), "/").map_err(zarr)?;
        let ome = group.attributes().get("ome").cloned().unwrap_or_default();
        let multiscale = &ome["multiscales"][0];
        let axes: Vec<&str> = multiscale["axes"]
            .as_array()
            .map(|axes| axes.iter().filter_map(|a| a["name"].as_str()).collect())
            .unwrap_or_default();
        if axes != ["c", "z", "y", "x"] {
            return Err(StoreError::Zarr(format!(
                "axes {axes:?}, where c, z, y, x are read"
            )));
        }
        let datasets = multiscale["datasets"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let arrays = datasets
            .iter()
            .map(|dataset| {
                let path = dataset["path"].as_str().unwrap_or_default();
                Array::open(Arc::clone(&storage), &format!("/{path}")).map_err(zarr)
            })
            .collect::<Result<Vec<_>>>()?;
        let first = arrays.first().ok_or_else(|| zarr("no levels"))?;
        let sample = match first.data_type() {
            DataType::UInt8 => SampleType::U8,
            DataType::UInt16 => SampleType::U16,
            DataType::Float32 => SampleType::F32,
            other => return Err(StoreError::Zarr(format!("samples of {other}"))),
        };
        let shape = first.shape();
        let scale = &datasets[0]["coordinateTransformations"][0]["scale"];
        let unit = multiscale["axes"][3]["unit"].as_str() == Some("micrometer");
        let pixel_size = unit.then(|| PixelSize {
            x: scale[3].as_f64().unwrap_or(1.0),
            y: scale[2].as_f64().unwrap_or(1.0),
            z: scale[1].as_f64().unwrap_or(1.0),
        });
        let names = ome["omero"]["channels"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let info = ImageInfo {
            width: shape[3],
            height: shape[2],
            channels: shape[0] as u32,
            planes: shape[1] as u32,
            sample,
            pixel_size,
            channel_names: names
                .iter()
                .filter_map(|c| c["label"].as_str().map(str::to_owned))
                .collect(),
            levels: arrays.len() as u32,
        };
        Ok(Self { info, arrays })
    }

    /// The image.
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// The samples, interleaved, of the pixels of plane `plane` of level
    /// `level` whose top-left is (`x`, `y`), `width` × `height` of them.
    pub fn read(
        &self,
        level: u32,
        plane: u32,
        (x, y): (u64, u64),
        (width, height): (u64, u64),
    ) -> Result<Samples> {
        let array = self.arrays.get(level as usize).ok_or_else(|| {
            StoreError::Refused(format!("level {level}: the image has {}", self.info.levels))
        })?;
        let region = Region {
            plane: u64::from(plane),
            x,
            y,
            width,
            height,
            channels: u64::from(self.info.channels),
        };
        Ok(match self.info.sample {
            SampleType::U8 => Samples::U8(retrieve(array, region)?),
            SampleType::U16 => Samples::U16(retrieve(array, region)?),
            SampleType::F32 => Samples::F32(retrieve(array, region)?),
        })
    }
}
