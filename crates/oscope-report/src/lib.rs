//! Reports (P47, ADR-0030): a report's model, and its rendering, to PDF by
//! Typst in process and to HTML by minijinja, through templates, built in
//! or the user's.
//!
//! A report is a title, its metadata, sections of text, images, tables and
//! charts, and its provenance. Its images come beside it, PNG or JPEG by
//! name; its charts are drawn as SVG. The fonts are Roboto's, Apache-2.0,
//! in the crate, so a report renders the same everywhere.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod chart;
mod render;
mod templates;

pub use chart::svg;
pub use render::{Pdf, from_base64, html, pdf};
pub use templates::{STANDARD, Template, Templates};

/// A report.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// Its title.
    pub title: String,
    /// What it is about, under its title.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Its date, as written: given, so that it renders the same again.
    pub date: String,
    /// What it is about, as named values: the project, the sample.
    #[serde(default)]
    pub metadata: Vec<Field>,
    /// Its sections.
    #[serde(default)]
    pub sections: Vec<Section>,
    /// How its results were made, as named values.
    #[serde(default)]
    pub provenance: Vec<Field>,
}

/// A named value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Its name.
    pub name: String,
    /// Its value, as written.
    pub value: String,
}

/// A section: a heading and its blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Section {
    /// Its heading.
    pub heading: String,
    /// Its blocks, in order.
    #[serde(default)]
    pub blocks: Vec<Block>,
}

/// A block of a section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    /// A paragraph.
    Text {
        /// Its text.
        text: String,
    },
    /// An image of the report's, by name.
    Image {
        /// Its name among the report's images.
        name: String,
        /// Its caption.
        #[serde(default)]
        caption: String,
        /// Its width, a share of the text's: 0.8 unless given.
        #[serde(default)]
        width: Option<f64>,
    },
    /// A table, its cells written.
    Table {
        /// Its caption.
        #[serde(default)]
        caption: String,
        /// Its columns' names.
        columns: Vec<String>,
        /// Its rows, a cell for each column.
        rows: Vec<Vec<String>>,
    },
    /// A chart, drawn as SVG.
    Chart {
        /// Its caption.
        #[serde(default)]
        caption: String,
        /// What it shows.
        chart: Chart,
    },
}

/// A chart: bars or a line, of values by their labels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Chart {
    /// Bars or a line.
    pub kind: ChartKind,
    /// Each value's label, along x.
    pub labels: Vec<String>,
    /// The values, up y, none negative.
    pub values: Vec<f64>,
    /// The x axis's name.
    #[serde(default)]
    pub x_label: String,
    /// The y axis's name.
    #[serde(default)]
    pub y_label: String,
}

/// How a chart draws its values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// A bar each.
    Bars,
    /// A line through them.
    Line,
}

/// A report's images: PNG or JPEG bytes, by name.
pub type Images = BTreeMap<String, Vec<u8>>;

/// Why a report didn't render.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ReportError {
    /// The template is wrong: where, and why.
    #[error("the {kind} template, line {line}, column {column}: {message}")]
    Template {
        /// `typst` or `html`.
        kind: &'static str,
        /// Its line, from 1; 0 if unknown.
        line: usize,
        /// Its column, from 1; 0 if unknown.
        column: usize,
        /// What is wrong.
        message: String,
    },
    /// The report can't be rendered as it is, saying why.
    #[error("{0}")]
    Report(String),
    /// A template's name, or a request about one, is refused.
    #[error("{0}")]
    Refused(String),
    /// A file failed.
    #[error("{0}")]
    Io(String),
}

/// A small report with a block of each kind and an image: what templates
/// are checked against.
pub fn sample() -> (Report, Images) {
    let field = |name: &str, value: &str| Field {
        name: name.to_owned(),
        value: value.to_owned(),
    };
    let report = Report {
        title: "A sample report".to_owned(),
        subtitle: Some("Every kind of block".to_owned()),
        date: "2026-01-01".to_owned(),
        metadata: vec![field("Project", "a project"), field("Sample", "a slide")],
        sections: vec![Section {
            heading: "Results".to_owned(),
            blocks: vec![
                Block::Text {
                    text: "Some text.".to_owned(),
                },
                Block::Image {
                    name: "dot.png".to_owned(),
                    caption: "An image".to_owned(),
                    width: Some(0.2),
                },
                Block::Table {
                    caption: "A table".to_owned(),
                    columns: vec!["a".to_owned(), "b".to_owned()],
                    rows: vec![vec!["1".to_owned(), "2".to_owned()]],
                },
                Block::Chart {
                    caption: "A chart".to_owned(),
                    chart: Chart {
                        kind: ChartKind::Bars,
                        labels: vec!["x".to_owned(), "y".to_owned()],
                        values: vec![1.0, 2.0],
                        x_label: "label".to_owned(),
                        y_label: "value".to_owned(),
                    },
                },
            ],
        }],
        provenance: vec![field("Software", "OScope")],
    };
    let images = Images::from([("dot.png".to_owned(), dot())]);
    (report, images)
}

/// A PNG of 2 × 2 grey pixels.
fn dot() -> Vec<u8> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 2, 2);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("a PNG's header");
    writer.write_image_data(&[128; 4]).expect("a PNG's pixels");
    drop(writer);
    png
}
