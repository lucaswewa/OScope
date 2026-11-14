//! Bringing images in and getting them out (P30, ADR-0037).
//!
//! - **Importers** read a file into a new acquisition, a plane for each of
//!   its pages, keeping its samples' type and its channels: TIFF, one page
//!   or a stack of them; PNG; and JPEG.
//! - **Exporters** write a record's image to its folder's `exports/`:
//!   OME-TIFF, every plane, with OME-XML's metadata; 16-bit TIFF, every
//!   plane; and PNG, the first plane.
//!
//! Extensions add others: `oscope-ext` makes theirs these traits.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Seek, Write};
use std::path::Path;

use oscope_core::{AcquisitionId, BayerPattern, PixelFormat, ProjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tiff::ColorType;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::encoder::{TiffEncoder, TiffKind, TiffValue, colortype};
use tiff::tags::Tag;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::image::{ImageInfo, ImageReader, PixelSize, SampleType, Samples};
use crate::records::{Acquisition, RecordKind};
use crate::store::Store;
use crate::{Result, StoreError, io, thumbnail};

/// A file format, as an importer or an exporter names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Format {
    /// Its ID, such as `ome-tiff`: one importer and one exporter each.
    pub id: String,
    /// Its name, for people.
    pub name: String,
    /// Its files' extensions, without the dot and in lower case, such as
    /// `ome.tif`. An exporter names its files with the first.
    pub extensions: Vec<String>,
}

impl Format {
    fn new(id: &str, name: &str, extensions: &[&str]) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            extensions: extensions.iter().map(|e| (*e).to_owned()).collect(),
        }
    }

    /// Whether `file` has one of its extensions.
    pub fn matches(&self, file: &str) -> bool {
        let file = file.to_ascii_lowercase();
        self.extensions
            .iter()
            .any(|e| file.ends_with(&format!(".{e}")))
    }
}

/// Reads files of a format into images.
pub trait Importer: Send + Sync {
    /// Its format.
    fn format(&self) -> Format;
    /// What the file at `path` holds: its planes' size, their samples and
    /// channels, and how many there are.
    fn probe(&self, path: &Path) -> Result<ImageInfo>;
    /// Plane `plane` of the file at `path`, which `info` describes:
    /// interleaved samples.
    fn read(&self, path: &Path, info: &ImageInfo, plane: u32) -> Result<Samples>;
}

/// Writes images to files of a format.
pub trait Exporter: Send + Sync {
    /// Its format.
    fn format(&self) -> Format;
    /// Writes the image `info` describes, named `name`, to `path`, taking
    /// each plane it needs from `plane`.
    fn write(
        &self,
        path: &Path,
        info: &ImageInfo,
        name: &str,
        plane: &mut dyn FnMut(u32) -> Result<Samples>,
    ) -> Result<()>;
}

/// The built-in importers: TIFF, PNG and JPEG.
pub fn importers() -> Vec<std::sync::Arc<dyn Importer>> {
    vec![
        std::sync::Arc::new(TiffImporter),
        std::sync::Arc::new(PngImporter),
        std::sync::Arc::new(JpegImporter),
    ]
}

/// The built-in exporters: OME-TIFF, 16-bit TIFF and PNG.
pub fn exporters() -> Vec<std::sync::Arc<dyn Exporter>> {
    vec![
        std::sync::Arc::new(TiffExporter { ome: true }),
        std::sync::Arc::new(TiffExporter { ome: false }),
        std::sync::Arc::new(PngExporter),
    ]
}

fn unreadable(path: &Path, reason: impl std::fmt::Display) -> StoreError {
    StoreError::Document {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

fn refused(reason: impl Into<String>) -> StoreError {
    StoreError::Refused(reason.into())
}

/// Samples of 8 bits stay 8 bits, of 16 stay 16, and others become floats.
fn sample_of(bits: u8) -> Option<SampleType> {
    match bits {
        8 => Some(SampleType::U8),
        16 => Some(SampleType::U16),
        32 | 64 => Some(SampleType::F32),
        _ => None,
    }
}

/// A TIFF page's samples and channels.
fn tiff_layout(path: &Path, color: ColorType) -> Result<(SampleType, u32)> {
    let (bits, channels) = match color {
        ColorType::Gray(bits) => (bits, 1),
        ColorType::GrayA(bits) => (bits, 2),
        ColorType::RGB(bits) => (bits, 3),
        ColorType::RGBA(bits) => (bits, 4),
        other => return Err(unreadable(path, format!("pixels of {other:?}"))),
    };
    let sample =
        sample_of(bits).ok_or_else(|| unreadable(path, format!("samples of {bits} bits")))?;
    Ok((sample, channels))
}

fn tiff_decoder(path: &Path) -> Result<Decoder<BufReader<File>>> {
    let file = BufReader::new(File::open(path).map_err(io(path))?);
    Decoder::new(file)
        .map(|decoder| decoder.with_limits(Limits::unlimited()))
        .map_err(|e| unreadable(path, e))
}

struct TiffImporter;

impl Importer for TiffImporter {
    fn format(&self) -> Format {
        Format::new("tiff", "TIFF", &["tif", "tiff"])
    }

    fn probe(&self, path: &Path) -> Result<ImageInfo> {
        let bad = |e: tiff::TiffError| unreadable(path, e);
        let mut decoder = tiff_decoder(path)?;
        let (width, height) = decoder.dimensions().map_err(bad)?;
        let layout = tiff_layout(path, decoder.colortype().map_err(bad)?)?;
        let description = decoder.get_tag_ascii_string(Tag::ImageDescription).ok();
        let mut planes = 1;
        while decoder.more_images() {
            decoder.next_image().map_err(bad)?;
            let size = decoder.dimensions().map_err(bad)?;
            if size != (width, height)
                || tiff_layout(path, decoder.colortype().map_err(bad)?)? != layout
            {
                return Err(refused(format!(
                    "page {} of {} differs from the first: a stack's pages share their size and samples",
                    planes + 1,
                    path.display()
                )));
            }
            planes += 1;
        }
        let mut info = ImageInfo::new(u64::from(width), u64::from(height), layout.1, layout.0);
        info.planes = planes;
        if let Some(ome) = description.as_deref().and_then(Ome::parse) {
            info.pixel_size = ome.pixel_size;
            if ome.names.len() == layout.1 as usize {
                info.channel_names = ome.names;
            }
        }
        Ok(info)
    }

    fn read(&self, path: &Path, info: &ImageInfo, plane: u32) -> Result<Samples> {
        let bad = |e: tiff::TiffError| unreadable(path, e);
        let mut decoder = tiff_decoder(path)?;
        for _ in 0..plane {
            decoder.next_image().map_err(bad)?;
        }
        let samples = match decoder.read_image().map_err(bad)? {
            DecodingResult::U8(v) => Samples::U8(v),
            DecodingResult::U16(v) => Samples::U16(v),
            DecodingResult::F32(v) => Samples::F32(v),
            DecodingResult::U32(v) => Samples::F32(v.into_iter().map(|s| s as f32).collect()),
            DecodingResult::F64(v) => Samples::F32(v.into_iter().map(|s| s as f32).collect()),
            _ => return Err(unreadable(path, "signed samples")),
        };
        match samples.sample_type() == info.sample {
            true => Ok(samples),
            false => Err(unreadable(path, format!("page {plane} has other samples"))),
        }
    }
}

fn png_reader(path: &Path) -> Result<png::Reader<BufReader<File>>> {
    let file = BufReader::new(File::open(path).map_err(io(path))?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND);
    decoder.read_info().map_err(|e| unreadable(path, e))
}

struct PngImporter;

impl Importer for PngImporter {
    fn format(&self) -> Format {
        Format::new("png", "PNG", &["png"])
    }

    fn probe(&self, path: &Path) -> Result<ImageInfo> {
        let reader = png_reader(path)?;
        let (color, depth) = reader.output_color_type();
        let sample = match depth {
            png::BitDepth::Sixteen => SampleType::U16,
            _ => SampleType::U8,
        };
        let info = reader.info();
        let channels = color.samples() as u32;
        Ok(ImageInfo::new(
            u64::from(info.width),
            u64::from(info.height),
            channels,
            sample,
        ))
    }

    fn read(&self, path: &Path, info: &ImageInfo, _plane: u32) -> Result<Samples> {
        let mut reader = png_reader(path)?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| unreadable(path, "too large"))?;
        let mut buffer = vec![0; size];
        let frame = reader
            .next_frame(&mut buffer)
            .map_err(|e| unreadable(path, e))?;
        buffer.truncate(frame.buffer_size());
        Ok(match info.sample {
            SampleType::U16 => Samples::U16(
                buffer
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_be_bytes(*b))
                    .collect(),
            ),
            _ => Samples::U8(buffer),
        })
    }
}

/// A JPEG's decoder, to grey if it is grey and to RGB otherwise.
fn jpeg(path: &Path) -> Result<(Vec<u8>, u32, u32, u32)> {
    let bytes = fs::read(path).map_err(io(path))?;
    let mut probe = JpegDecoder::new(Cursor::new(&bytes));
    probe.decode_headers().map_err(|e| unreadable(path, e))?;
    let grey = probe.input_colorspace() == Some(ColorSpace::Luma);
    let (space, channels) = if grey {
        (ColorSpace::Luma, 1)
    } else {
        (ColorSpace::RGB, 3)
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(space);
    let mut decoder = JpegDecoder::new_with_options(Cursor::new(&bytes), options);
    let pixels = decoder.decode().map_err(|e| unreadable(path, e))?;
    let info = decoder.info().ok_or_else(|| unreadable(path, "no image"))?;
    Ok((
        pixels,
        u32::from(info.width),
        u32::from(info.height),
        channels,
    ))
}

struct JpegImporter;

impl Importer for JpegImporter {
    fn format(&self) -> Format {
        Format::new("jpeg", "JPEG", &["jpg", "jpeg"])
    }

    fn probe(&self, path: &Path) -> Result<ImageInfo> {
        let (_, width, height, channels) = jpeg(path)?;
        Ok(ImageInfo::new(
            u64::from(width),
            u64::from(height),
            channels,
            SampleType::U8,
        ))
    }

    fn read(&self, path: &Path, _info: &ImageInfo, _plane: u32) -> Result<Samples> {
        Ok(Samples::U8(jpeg(path)?.0))
    }
}

/// What OScope's OME-XML says, read back.
struct Ome {
    pixel_size: Option<PixelSize>,
    names: Vec<String>,
}

/// The value of the first attribute `name` after `from` in `xml`.
fn attribute<'a>(xml: &'a str, from: &str, name: &str) -> Option<&'a str> {
    let at = xml.find(from)?;
    let rest = &xml[at..];
    let start = rest.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = rest[start..].find('"')?;
    Some(&rest[start..start + end])
}

/// `text` for an XML attribute, in ASCII, as TIFF's text is: other
/// characters as references.
fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            c if c.is_ascii() => c.to_string(),
            c => format!("&#x{:X};", u32::from(c)),
        })
        .collect()
}

/// An XML attribute's text, its references read.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find(';') else {
            break;
        };
        let name = &rest[at + 1..at + end];
        let code = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match code {
            Some(c) => out.push(c),
            None => out.push_str(&rest[at..=at + end]),
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out
}

impl Ome {
    /// The OME-XML of `info`, named `name`: its planes on `Z`, its
    /// channels interleaved as one channel of their samples.
    fn xml(info: &ImageInfo, name: &str) -> String {
        let kind = match info.sample {
            SampleType::U8 => "uint8",
            SampleType::U16 => "uint16",
            SampleType::F32 => "float",
        };
        // In µm, the schema's unit when none is given: TIFF's text is ASCII.
        let physical = info.pixel_size.map_or_else(String::new, |p| {
            format!(
                " PhysicalSizeX=\"{}\" PhysicalSizeY=\"{}\" PhysicalSizeZ=\"{}\"",
                p.x, p.y, p.z
            )
        });
        let schema = "http://www.openmicroscopy.org/Schemas/OME/2016-06";
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <OME xmlns=\"{schema}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
             xsi:schemaLocation=\"{schema} {schema}/ome.xsd\" Creator=\"OScope {version}\">\
             <Image ID=\"Image:0\" Name=\"{name}\">\
             <Pixels ID=\"Pixels:0\" DimensionOrder=\"XYCZT\" Type=\"{kind}\" SizeX=\"{w}\" \
             SizeY=\"{h}\" SizeC=\"{c}\" SizeZ=\"{z}\" SizeT=\"1\" Interleaved=\"true\" \
             BigEndian=\"false\"{physical}>\
             <Channel ID=\"Channel:0:0\" Name=\"{names}\" SamplesPerPixel=\"{c}\"/>\
             <TiffData IFD=\"0\" PlaneCount=\"{z}\"/>\
             </Pixels></Image></OME>",
            version = env!("CARGO_PKG_VERSION"),
            name = escape(name),
            w = info.width,
            h = info.height,
            c = info.channels,
            z = info.planes,
            names = escape(&info.channel_names.join(", ")),
        )
    }

    /// What `xml` says, if it is OME-XML.
    fn parse(xml: &str) -> Option<Ome> {
        if !xml.contains("<OME") {
            return None;
        }
        let size = |axis: &str| {
            attribute(xml, "<Pixels", &format!("PhysicalSize{axis}"))?
                .parse()
                .ok()
        };
        let pixel_size = match (size("X"), size("Y")) {
            (Some(x), Some(y)) => Some(PixelSize {
                x,
                y,
                z: size("Z").unwrap_or(1.0),
            }),
            _ => None,
        };
        let names = attribute(xml, "<Channel", "Name")
            .map(|n| unescape(n).split(", ").map(str::to_owned).collect())
            .unwrap_or_default();
        Some(Ome { pixel_size, names })
    }
}

/// `samples` as 16 bits: 8 bits times 257, and floats from 0–1, clamped.
fn to_u16(samples: Samples) -> Vec<u16> {
    match samples {
        Samples::U8(v) => v.into_iter().map(|s| u16::from(s) * 257).collect(),
        Samples::U16(v) => v,
        Samples::F32(v) => v
            .into_iter()
            .map(|s| (s.clamp(0.0, 1.0) * 65_535.0).round() as u16)
            .collect(),
    }
}

/// Writes one page of `C`, with `description` if any.
fn page<W, K, C>(
    encoder: &mut TiffEncoder<W, K>,
    (width, height): (u32, u32),
    data: &[C::Inner],
    description: Option<&str>,
) -> tiff::TiffResult<()>
where
    W: Write + Seek,
    K: TiffKind,
    C: colortype::ColorType,
    [C::Inner]: TiffValue,
{
    let mut image = encoder.new_image::<C>(width, height)?;
    if let Some(description) = description {
        image
            .encoder()
            .write_tag(Tag::ImageDescription, description)?;
    }
    image.write_data(data)
}

/// Writes every plane as a page, 16 bits if `sixteen`, the first with
/// `description`.
fn pages<W: Write + Seek, K: TiffKind>(
    path: &Path,
    mut encoder: TiffEncoder<W, K>,
    info: &ImageInfo,
    description: Option<&str>,
    sixteen: bool,
    plane: &mut dyn FnMut(u32) -> Result<Samples>,
) -> Result<()> {
    let size = (info.width as u32, info.height as u32);
    let bad = |e: tiff::TiffError| unreadable(path, format!("a TIFF: {e}"));
    for p in 0..info.planes {
        let mut samples = plane(p)?;
        if sixteen {
            samples = Samples::U16(to_u16(samples));
        }
        let text = description.filter(|_| p == 0);
        let e = &mut encoder;
        match (&samples, info.channels) {
            (Samples::U8(v), 1) => page::<_, _, colortype::Gray8>(e, size, v, text),
            (Samples::U8(v), 3) => page::<_, _, colortype::RGB8>(e, size, v, text),
            (Samples::U8(v), _) => page::<_, _, colortype::RGBA8>(e, size, v, text),
            (Samples::U16(v), 1) => page::<_, _, colortype::Gray16>(e, size, v, text),
            (Samples::U16(v), 3) => page::<_, _, colortype::RGB16>(e, size, v, text),
            (Samples::U16(v), _) => page::<_, _, colortype::RGBA16>(e, size, v, text),
            (Samples::F32(v), 1) => page::<_, _, colortype::Gray32Float>(e, size, v, text),
            (Samples::F32(v), 3) => page::<_, _, colortype::RGB32Float>(e, size, v, text),
            (Samples::F32(v), _) => page::<_, _, colortype::RGBA32Float>(e, size, v, text),
        }
        .map_err(bad)?;
    }
    Ok(())
}

/// OME-TIFF, or a plain 16-bit TIFF.
struct TiffExporter {
    ome: bool,
}

impl Exporter for TiffExporter {
    fn format(&self) -> Format {
        match self.ome {
            true => Format::new("ome-tiff", "OME-TIFF", &["ome.tif", "ome.tiff"]),
            false => Format::new("tiff16", "16-bit TIFF", &["tif", "tiff"]),
        }
    }

    fn write(
        &self,
        path: &Path,
        info: &ImageInfo,
        name: &str,
        plane: &mut dyn FnMut(u32) -> Result<Samples>,
    ) -> Result<()> {
        if !matches!(info.channels, 1 | 3 | 4) {
            return Err(refused(format!(
                "a TIFF holds 1, 3 or 4 channels, not {}",
                info.channels
            )));
        }
        let sample_bytes = match (self.ome, info.sample) {
            (false, _) | (true, SampleType::U16) => 2,
            (true, SampleType::U8) => 1,
            (true, SampleType::F32) => 4,
        };
        let bytes = info.width * info.height * u64::from(info.channels) * u64::from(info.planes);
        let file = BufWriter::new(File::create(path).map_err(io(path))?);
        let xml = self.ome.then(|| Ome::xml(info, name));
        let description = xml.as_deref();
        let bad = |e: tiff::TiffError| unreadable(path, format!("a TIFF: {e}"));
        // Classic TIFF's offsets are 32 bits: over 4 GB, BigTIFF.
        match bytes * sample_bytes > 4_000_000_000 {
            true => pages(
                path,
                TiffEncoder::new_big(file).map_err(bad)?,
                info,
                description,
                !self.ome,
                plane,
            ),
            false => pages(
                path,
                TiffEncoder::new(file).map_err(bad)?,
                info,
                description,
                !self.ome,
                plane,
            ),
        }
    }
}

/// PNG: the first plane, in 8 bits or 16.
struct PngExporter;

impl Exporter for PngExporter {
    fn format(&self) -> Format {
        Format::new("png", "PNG", &["png"])
    }

    fn write(
        &self,
        path: &Path,
        info: &ImageInfo,
        _name: &str,
        plane: &mut dyn FnMut(u32) -> Result<Samples>,
    ) -> Result<()> {
        let color = match info.channels {
            1 => png::ColorType::Grayscale,
            2 => png::ColorType::GrayscaleAlpha,
            3 => png::ColorType::Rgb,
            _ => png::ColorType::Rgba,
        };
        let (depth, data) = match plane(0)? {
            Samples::U8(v) => (png::BitDepth::Eight, v),
            other => (
                png::BitDepth::Sixteen,
                to_u16(other)
                    .into_iter()
                    .flat_map(u16::to_be_bytes)
                    .collect(),
            ),
        };
        let file = BufWriter::new(File::create(path).map_err(io(path))?);
        let mut encoder = png::Encoder::new(file, info.width as u32, info.height as u32);
        encoder.set_color(color);
        encoder.set_depth(depth);
        let bad = |e: png::EncodingError| StoreError::Refused(format!("a PNG: {e}"));
        let mut writer = encoder.write_header().map_err(bad)?;
        writer.write_image_data(&data).map_err(bad)?;
        writer.finish().map_err(bad)
    }
}

/// `name` as a file's name: letters, digits, spaces, `-`, `_` and `.`, the
/// rest as `-`.
fn file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(
            |c| match c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                true => c,
                false => '-',
            },
        )
        .collect();
    match safe.trim().trim_matches('.') {
        "" => "image".to_owned(),
        trimmed => trimmed.to_owned(),
    }
}

/// Imports the file at `path` with `importer` into a new acquisition of
/// `project`, named `name` or as the file: a plane for each of its pages.
pub fn import(
    store: &Store,
    project: ProjectId,
    importer: &dyn Importer,
    path: &Path,
    name: Option<&str>,
) -> Result<Acquisition> {
    let info = importer.probe(path)?;
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = file.split('.').next().unwrap_or("image").to_owned();
    let mut draft = store.acquisition(project, name.unwrap_or(&stem), &info)?;
    draft.record().instrument = json!({
        "imported": { "file": file, "format": importer.format().id },
    });
    let Some(image) = draft.image() else {
        unreachable!("an acquisition has an image");
    };
    for plane in 0..info.planes {
        let samples = importer.read(path, &info, plane)?;
        image.write(plane, (0, 0), (info.width, info.height), &samples)?;
    }
    draft.commit()
}

/// A record's folder, and its image's store, if it has one.
fn record_image(
    store: &Store,
    project: ProjectId,
    kind: RecordKind,
    id: &str,
) -> Result<(std::path::PathBuf, String)> {
    if uuid::Uuid::try_parse(id).is_err() {
        return Err(refused(format!("{id:?} isn't a record's ID")));
    }
    let contents = store.contents(project)?;
    let missing = || StoreError::NotFound {
        kind: kind.name(),
        id: id.to_owned(),
    };
    let (name, image) = match kind {
        RecordKind::Acquisition => contents
            .acquisitions
            .iter()
            .find(|a| a.id.to_string() == id)
            .map(|a| (a.name.clone(), "raw.ome.zarr"))
            .ok_or_else(missing)?,
        RecordKind::Result => {
            let result = contents
                .results
                .iter()
                .find(|r| r.id.to_string() == id)
                .ok_or_else(missing)?;
            match result.image {
                Some(_) => (result.name.clone(), "image.ome.zarr"),
                None => return Err(refused(format!("result {id} has no image"))),
            }
        }
        RecordKind::Report => return Err(refused("a report has no image to export")),
    };
    let folder = store.folder(project).join(kind.folder()).join(id);
    Ok((folder.join(image), name))
}

/// Exports a record's image with `exporter` to its folder's `exports/`,
/// named as the record: the file's path in the project's folder, as the
/// data route serves it.
pub fn export(
    store: &Store,
    project: ProjectId,
    kind: RecordKind,
    id: &str,
    exporter: &dyn Exporter,
) -> Result<String> {
    let (image, name) = record_image(store, project, kind, id)?;
    let reader = ImageReader::open(&image)?;
    let info = reader.info().clone();
    let format = exporter.format();
    let extension = format.extensions.first().map_or("bin", String::as_str);
    let file = format!("{}.{extension}", file_name(&name));
    let exports = store
        .folder(project)
        .join(kind.folder())
        .join(id)
        .join("exports");
    fs::create_dir_all(&exports).map_err(io(&exports))?;
    let (path, temporary) = (exports.join(&file), exports.join(format!(".{file}.part")));
    let mut plane = |p: u32| reader.read(0, p, (0, 0), (info.width, info.height));
    if let Err(error) = exporter.write(&temporary, &info, &name, &mut plane) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).map_err(io(&path))?;
    Ok(format!("{}/{id}/exports/{file}", kind.folder()))
}

/// How a rendition is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Rendition {
    /// PNG, lossless.
    Png,
    /// JPEG, at quality 90.
    Jpeg,
}

/// Writes an acquisition's display rendition, `rendition.png` or `.jpg`,
/// to its folder: its first plane at full resolution, demosaiced if its
/// camera gave a Bayer mosaic, and shown as its thumbnail is. The file's
/// path in the project's folder, as the data route serves it.
pub fn render(
    store: &Store,
    project: ProjectId,
    acquisition: AcquisitionId,
    rendition: Rendition,
) -> Result<String> {
    let id = acquisition.to_string();
    let (image, _) = record_image(store, project, RecordKind::Acquisition, &id)?;
    let contents = store.contents(project)?;
    let record = contents.acquisitions.iter().find(|a| a.id == acquisition);
    let bayer: Option<BayerPattern> = record
        .and_then(|a| serde_json::from_value::<PixelFormat>(a.camera["format"].clone()).ok())
        .and_then(PixelFormat::bayer);
    let reader = ImageReader::open(&image)?;
    let file = match rendition {
        Rendition::Png => "rendition.png",
        Rendition::Jpeg => "rendition.jpg",
    };
    let out = store
        .folder(project)
        .join("acquisitions")
        .join(&id)
        .join(file);
    thumbnail::rendition(&reader, bayer, rendition == Rendition::Png, &out)?;
    Ok(format!("acquisitions/{id}/{file}"))
}
