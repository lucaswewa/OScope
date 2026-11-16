//! Rendering: to PDF by Typst, in process, through a world of the
//! template, the report as JSON, its images, its charts and Roboto; and to
//! HTML by minijinja, its images inline and its charts as SVG.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use minijinja::value::Serde;
use minijinja::{Environment, context};
use serde_json::{Value, json};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::layout::{Frame, FrameItem};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_layout::PagedDocument;

use crate::{Block, Images, Report, ReportError, chart};

/// Roboto, regular, bold and italic.
static FONTS: LazyLock<Vec<Font>> = LazyLock::new(|| {
    let files: [&'static [u8]; 3] = [
        include_bytes!("../fonts/Roboto-Regular.ttf"),
        include_bytes!("../fonts/Roboto-Bold.ttf"),
        include_bytes!("../fonts/Roboto-Italic.ttf"),
    ];
    files
        .into_iter()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect()
});
static BOOK: LazyLock<LazyHash<FontBook>> =
    LazyLock::new(|| LazyHash::new(FontBook::from_fonts(FONTS.iter())));
static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

/// The report as its templates read it, each chart given a file's name;
/// and the charts' SVG by those names.
fn prepared(report: &Report) -> Result<(Value, BTreeMap<String, String>), ReportError> {
    let mut value = serde_json::to_value(report).map_err(|e| ReportError::Report(e.to_string()))?;
    let mut charts = BTreeMap::new();
    for (s, section) in report.sections.iter().enumerate() {
        for (b, block) in section.blocks.iter().enumerate() {
            if let Block::Chart { chart, .. } = block {
                let file = format!("chart-{}.svg", charts.len() + 1);
                value["sections"][s]["blocks"][b]["file"] = json!(file);
                charts.insert(file, chart::svg(chart));
            }
        }
    }
    Ok((value, charts))
}

/// The file at `path`, from the project's root.
fn at(path: &str) -> FileId {
    let path = VirtualPath::new(path).expect("a file's path in the report's world");
    RootedPath::new(VirtualRoot::Project, path).intern()
}

/// The world a report compiles in: its template, its files, the fonts.
struct Sandbox {
    main: Source,
    files: HashMap<FileId, Bytes>,
    today: Option<Datetime>,
}

impl World for Sandbox {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &BOOK
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        match id == self.main.id() {
            true => Ok(self.main.clone()),
            false => Err(FileError::NotSource),
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            return Ok(Bytes::from_string(self.main.clone()));
        }
        let missing = || FileError::Other(Some("the report has no such file".into()));
        self.files.get(&id).cloned().ok_or_else(missing)
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.get(index).cloned()
    }

    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        self.today
    }
}

/// What a report rendered as PDF made: the PDF, and each page's text.
#[derive(Clone, Debug)]
pub struct Pdf {
    /// The PDF.
    pub bytes: Vec<u8>,
    /// Each page's text, its runs joined by spaces: for checking.
    pub pages: Vec<String>,
}

/// The text in `frame`, its runs joined by spaces.
fn text(frame: &Frame, out: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => text(&group.frame, out),
            FrameItem::Text(run) => {
                out.push_str(&run.text);
                out.push(' ');
            }
            _ => {}
        }
    }
}

/// The first of `errors`, where it is in the template.
fn template_error(world: &Sandbox, errors: &[SourceDiagnostic]) -> ReportError {
    let Some(first) = errors.first() else {
        return ReportError::Report("Typst failed, saying nothing".to_owned());
    };
    let inside = first.span.id() == Some(world.main.id());
    let place = inside.then(|| world.range(first.span)).flatten();
    let place = place.and_then(|range| world.main.lines().byte_to_line_column(range.start));
    let (line, column) = place.map_or((0, 0), |(l, c)| (l + 1, c + 1));
    let hints: Vec<&str> = first.hints.iter().map(|h| h.v.as_str()).collect();
    let mut message = first.message.to_string();
    if !hints.is_empty() {
        message = format!("{message} ({})", hints.join("; "));
    }
    ReportError::Template {
        kind: "typst",
        line,
        column,
        message,
    }
}

/// `report` and its `images` through the Typst `template`: a PDF.
pub fn pdf(template: &str, report: &Report, images: &Images) -> Result<Pdf, ReportError> {
    let (value, charts) = prepared(report)?;
    let main = Source::new(at("/main.typ"), template.to_owned());
    let json = serde_json::to_vec(&value).map_err(|e| ReportError::Report(e.to_string()))?;
    let mut files = HashMap::from([(at("/report.json"), Bytes::new(json))]);
    for (name, data) in images {
        files.insert(at(&format!("/images/{name}")), Bytes::new(data.clone()));
    }
    for (name, svg) in charts {
        files.insert(at(&format!("/charts/{name}")), Bytes::new(svg.into_bytes()));
    }
    let day = |k: usize| {
        report
            .date
            .split('-')
            .nth(k)
            .and_then(|v| v.parse::<i32>().ok())
    };
    let today = match (day(0), day(1), day(2)) {
        (Some(y), Some(m), Some(d)) => Datetime::from_ymd(y, m as u8, d as u8),
        _ => None,
    };
    let world = Sandbox { main, files, today };
    let document: PagedDocument = typst::compile(&world)
        .output
        .map_err(|errors| template_error(&world, &errors))?;
    let options = typst_pdf::PdfOptions::default();
    let bytes =
        typst_pdf::pdf(&document, &options).map_err(|errors| template_error(&world, &errors))?;
    let pages = document
        .pages()
        .iter()
        .map(|page| {
            let mut out = String::new();
            text(&page.frame, &mut out);
            out
        })
        .collect();
    Ok(Pdf { bytes, pages })
}

/// `data` as a `data:` URI, PNG or JPEG by its first bytes.
fn data_uri(data: &[u8]) -> String {
    const SIXTY_FOUR: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let kind = if data.starts_with(&[0xff, 0xd8]) {
        "jpeg"
    } else {
        "png"
    };
    let mut out = format!("data:image/{kind};base64,");
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            match k <= chunk.len() {
                true => out.push(SIXTY_FOUR[(n >> (18 - 6 * k) & 63) as usize] as char),
                false => out.push('='),
            }
        }
    }
    out
}

/// The bytes `text` holds in base64, padded or not: None if it isn't.
pub fn from_base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = text
        .trim_end_matches('=')
        .bytes()
        .map(value)
        .collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |n, (k, &d)| n | u32::from(d) << (18 - 6 * k));
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

/// `report` and its `images` through the minijinja `template`: HTML, its
/// images inline and its charts as SVG.
pub fn html(template: &str, report: &Report, images: &Images) -> Result<String, ReportError> {
    let (value, charts) = prepared(report)?;
    let wrong = |e: minijinja::Error| ReportError::Template {
        kind: "html",
        line: e.line().unwrap_or(0),
        column: 0,
        message: e.to_string(),
    };
    let mut environment = Environment::new();
    environment
        .add_template("report.html", template)
        .map_err(wrong)?;
    let uris: BTreeMap<&String, String> = images.iter().map(|(n, d)| (n, data_uri(d))).collect();
    let page = environment.get_template("report.html").map_err(wrong)?;
    page.render(context! {
        report => minijinja::Value::from(Serde(&value)),
        images => minijinja::Value::from(Serde(&uris)),
        charts => minijinja::Value::from(Serde(&charts)),
    })
    .map_err(wrong)
}
