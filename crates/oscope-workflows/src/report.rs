//! Reports as a job (P47): a report on a project's results, made by
//! OScope's own generator, `results`, or an extension's, rendered through a
//! template to PDF and HTML: a report of the project.
//!
//! OScope's generator writes a section for each result: its pictures, its
//! thumbnail and a region at full size, or its input's thumbnail; its
//! `summary.json` as a table; and for a count of cells, its nuclei's areas
//! as a chart and its first rows.

use std::sync::Arc;

use chrono::Utc;
use oscope_compute::{Image, Shape};
use oscope_core::ProjectId;
use oscope_imaging::color::{DisplayTable, encode_jpeg};
use oscope_imaging::zoom::Pyramid;
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_report::{
    Block, Chart, ChartKind, Field, Images, Report, STANDARD, Section, Templates, from_base64,
    html, pdf, sample,
};
use oscope_store::zoom::{ChunkCache, StoredImage, StoredPyramid, default_display, record_image};
use oscope_store::{Input, RecordKind, Store, StoreError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The job's kind.
pub const MAKE: &str = "report.make";

/// OScope's own generator's name.
pub const RESULTS: &str = "results";

/// A region's size at most, in pixels.
const REGION: (u64, u64) = (768, 480);

/// The extensions' report generators: by name, a report for its parameters.
pub type Generators = Arc<dyn Fn(&str, &Value) -> Result<Value, String> + Send + Sync>;

/// A report's form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// PDF, by Typst.
    Pdf,
    /// HTML, by minijinja.
    Html,
}

/// A report's options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ReportOptions {
    /// The generator: `results`, OScope's own, or an extension's by name.
    pub generator: String,
    /// The generator's parameters: for `results`, `{"results": [ids]}`, the
    /// project's results unless given.
    pub params: Value,
    /// The template: `standard` unless given.
    pub template: String,
    /// Its forms: PDF and HTML unless given.
    pub formats: Vec<Format>,
    /// Its date, as written: today's unless given.
    pub date: Option<String>,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            generator: RESULTS.to_owned(),
            params: Value::Null,
            template: STANDARD.to_owned(),
            formats: vec![Format::Pdf, Format::Html],
            date: None,
        }
    }
}

/// `value` written for a table: numbers to four decimals at most.
fn written(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) if n.is_f64() => {
            let s = format!("{:.4}", n.as_f64().unwrap_or(f64::NAN));
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        other => other.to_string(),
    }
}

/// `value`'s scalars, by their paths: `dab.counts.weak`.
fn flattened(prefix: &str, value: &Value, out: &mut Vec<Vec<String>>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flattened(&path, inner, out);
            }
        }
        Value::Array(items) if items.iter().all(|i| !i.is_object() && !i.is_array()) => {
            let joined: Vec<String> = items.iter().map(written).collect();
            out.push(vec![prefix.to_owned(), joined.join(", ")]);
        }
        Value::Array(_) => {}
        Value::Null => out.push(vec![prefix.to_owned(), "none".to_owned()]),
        other => out.push(vec![prefix.to_owned(), written(other)]),
    }
}

/// A count's nuclei: their areas' distribution in ten bins, and its first
/// ten rows.
fn cells_blocks(csv: &str) -> Vec<Block> {
    let mut lines = csv.split("\r\n").filter(|l| !l.is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let columns: Vec<String> = header.split(',').map(str::to_owned).collect();
    let rows: Vec<Vec<String>> = lines
        .map(|l| l.split(',').map(str::to_owned).collect())
        .collect();
    let area = columns.iter().position(|c| c == "area_um2");
    let areas: Vec<f64> = rows
        .iter()
        .filter_map(|r| area.and_then(|a| r.get(a)?.parse().ok()))
        .collect();
    let mut blocks = Vec::new();
    let most = areas.iter().copied().fold(0.0, f64::max);
    if !areas.is_empty() && most > 0.0 {
        let width = (most / 10.0).ceil().max(1.0);
        let mut counts = vec![0.0; 10];
        for a in &areas {
            counts[((a / width) as usize).min(9)] += 1.0;
        }
        let labels = (0..10)
            .map(|k| format!("{}", (k as f64 + 0.5) * width))
            .collect();
        blocks.push(Block::Chart {
            caption: format!("The areas of {} nuclei", areas.len()),
            chart: Chart {
                kind: ChartKind::Bars,
                labels,
                values: counts,
                x_label: "area (µm²), the bin's middle".to_owned(),
                y_label: "nuclei".to_owned(),
            },
        });
    }
    let first: Vec<Vec<String>> = rows
        .iter()
        .take(10)
        .map(|r| {
            r.iter()
                .map(|c| written(&serde_json::from_str(c).unwrap_or(Value::String(c.clone()))))
                .collect()
        })
        .collect();
    blocks.push(Block::Table {
        caption: format!(
            "The first {} of {} rows of cells.csv",
            first.len(),
            rows.len()
        ),
        columns,
        rows: first,
    });
    blocks
}

/// Result `id`'s image of `project`: a region of [`REGION`] at most at its
/// middle plane's middle, at full size, as its deep zoom shows it (P38), a
/// raw mosaic in colour; as JPEG, and what it covers.
fn region(store: &Store, project: ProjectId, id: &str) -> Result<(Vec<u8>, String), StoreError> {
    let failed = |e: String| StoreError::Zarr(format!("a report's region: {e}"));
    let (path, pattern) = record_image(store, project, RecordKind::Result, id)?;
    let planes = oscope_store::ImageReader::open(&path)?.info().planes;
    let image = Arc::new(StoredImage::open(&path, planes / 2)?);
    let (display, info) = (default_display(&image)?, image.info().clone());
    let cache = Arc::new(ChunkCache::new(64 << 20));
    let pyramid = StoredPyramid::new(image, cache, pattern.map(|p| (p, None)))?;
    let (w, h) = (REGION.0.min(info.width), REGION.1.min(info.height));
    let at = ((info.width - w) / 2, (info.height - h) / 2);
    let samples = pyramid
        .read(0, at, (w, h))
        .map_err(|e| failed(e.to_string()))?;
    let channels = pyramid.channels() as usize;
    let image = Image::from_vec(Shape::new(w as u32, h as u32, channels), samples)
        .map_err(|e| failed(e.to_string()))?;
    let table = DisplayTable::new(&display).map_err(|e| failed(e.to_string()))?;
    let shown = table.apply(&image);
    let rgb: Vec<u8> = match channels {
        3 => shown,
        _ => shown
            .iter()
            .step_by(channels)
            .flat_map(|&g| [g; 3])
            .collect(),
    };
    let jpeg = encode_jpeg(&rgb, w as u32, h as u32, 90).map_err(|e| failed(e.to_string()))?;
    let covers = match info.pixel_size {
        Some(size) => format!("{:.0} × {:.0} µm", w as f64 * size.x, h as f64 * size.y),
        None => format!("{w} × {h} pixels"),
    };
    Ok((jpeg, covers))
}

/// OScope's own report on `project`'s results: `params.results`' IDs, or
/// every one.
fn results_report(
    store: &Store,
    project: ProjectId,
    params: &Value,
    date: String,
) -> Result<(Report, Images), StoreError> {
    let contents = store.contents(project)?;
    let wanted: Option<Vec<&str>> = params["results"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect());
    let results: Vec<_> = contents
        .results
        .iter()
        .filter(|r| {
            wanted
                .as_ref()
                .is_none_or(|w| w.contains(&r.id.to_string().as_str()))
        })
        .collect();
    if results.is_empty() {
        return Err(StoreError::Refused(format!(
            "project {project} has no such results to report on"
        )));
    }
    let field = |name: &str, value: String| Field {
        name: name.to_owned(),
        value,
    };
    let p = &contents.project;
    let mut report = Report {
        title: format!("{}: results", p.name),
        subtitle: (!p.sample.is_empty()).then(|| p.sample.clone()),
        date,
        metadata: vec![
            field("Project", p.name.clone()),
            field("Results", results.len().to_string()),
        ],
        ..Report::default()
    };
    let mut images = Images::new();
    for r in results {
        let id = r.id.to_string();
        let mut blocks = Vec::new();
        // Its pictures: its thumbnail and a region at full size, or its
        // input's thumbnail.
        let input = r.provenance.inputs.first().map(|i| match i {
            Input::Acquisition(a) => a.to_string(),
            Input::Result(r) => r.to_string(),
        });
        let (own, thumbnail) = match store.thumbnail(project, Some(&id)) {
            Ok(path) => (true, Ok(path)),
            Err(e) => (
                false,
                input
                    .as_deref()
                    .map_or(Err(e), |i| store.thumbnail(project, Some(i))),
            ),
        };
        if let Ok(bytes) = thumbnail
            .and_then(|path| std::fs::read(&path).map_err(|source| StoreError::Io { path, source }))
        {
            let name = format!("{id}.jpg");
            images.insert(name.clone(), bytes);
            let caption = match own {
                true => format!("{}, whole", r.name),
                false => format!("What {} was made from", r.name),
            };
            blocks.push(Block::Image {
                name,
                caption,
                width: Some(0.6),
            });
        }
        if let Some((jpeg, covers)) = own.then(|| region(store, project, &id).ok()).flatten() {
            let name = format!("{id}-region.jpg");
            images.insert(name.clone(), jpeg);
            blocks.push(Block::Image {
                name,
                caption: format!("A region of {covers} at its middle, at full size"),
                width: Some(0.9),
            });
        }
        let folder = store.folder(project).join("results").join(&id);
        if let Ok(text) = std::fs::read_to_string(folder.join("summary.json")) {
            let mut rows = Vec::new();
            flattened(
                "",
                &serde_json::from_str(&text).unwrap_or(Value::Null),
                &mut rows,
            );
            blocks.push(Block::Table {
                caption: "Its summary".to_owned(),
                columns: vec!["Measure".to_owned(), "Value".to_owned()],
                rows,
            });
        }
        if let Ok(csv) = std::fs::read_to_string(folder.join("cells.csv")) {
            blocks.extend(cells_blocks(&csv));
        }
        let nodes: Vec<String> = r
            .provenance
            .nodes
            .iter()
            .map(|n| format!("{} {}", n.node, n.version))
            .collect();
        report.provenance.push(field(
            &r.name,
            format!("{}, by {}", r.provenance.software, nodes.join(", ")),
        ));
        report.sections.push(Section {
            heading: r.name.clone(),
            blocks,
        });
    }
    Ok((report, images))
}

/// An extension's report: its model, and its images from base64.
fn generated(made: Value) -> Result<(Report, Images), StoreError> {
    let refused = |why: String| StoreError::Refused(format!("the generator's report: {why}"));
    let mut made = made;
    let pictures = made
        .as_object_mut()
        .and_then(|m| m.remove("images"))
        .unwrap_or(Value::Null);
    let report: Report = serde_json::from_value(made).map_err(|e| refused(e.to_string()))?;
    let mut images = Images::new();
    for (name, data) in pictures.as_object().into_iter().flatten() {
        let bytes = data
            .as_str()
            .and_then(from_base64)
            .ok_or_else(|| refused(format!("image {name} isn't base64")))?;
        images.insert(name.clone(), bytes);
    }
    Ok((report, images))
}

/// A template's preview (W7.9): OScope's report on `result` of `project`,
/// or the sample report without one, through `typst` and `html`, kept
/// nowhere: the PDF's bytes and the HTML. A template's error is refused,
/// saying where it is.
pub fn preview(
    store: &Store,
    on: Option<(ProjectId, &str)>,
    typst: &str,
    html_source: &str,
) -> Result<(Vec<u8>, String), StoreError> {
    let refused = |e: oscope_report::ReportError| StoreError::Refused(e.to_string());
    let (report, images) = match on {
        Some((project, result)) => {
            let date = Utc::now().format("%Y-%m-%d").to_string();
            results_report(store, project, &json!({ "results": [result] }), date)?
        }
        None => sample(),
    };
    let made = pdf(typst, &report, &images).map_err(refused)?;
    Ok((
        made.bytes,
        html(html_source, &report, &images).map_err(refused)?,
    ))
}

/// Makes a report on `project` as `options` say, through `templates`, by
/// OScope's generator or one of `generators`: a report of the project,
/// `report.pdf`, `report.html` and `model.json` as its files.
pub fn make(
    store: &Store,
    project: ProjectId,
    options: &ReportOptions,
    templates: &Templates,
    generators: Option<&Generators>,
    progress: &dyn Fn(f64, String) -> bool,
) -> Result<oscope_store::Report, StoreError> {
    let refused = |why: String| StoreError::Refused(why);
    let date = options
        .date
        .clone()
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
    let (mut report, images) = match options.generator.as_str() {
        RESULTS => results_report(store, project, &options.params, date.clone())?,
        name => {
            let generate = generators.ok_or_else(|| {
                refused(format!(
                    "no report generator {name}: extensions aren't loaded"
                ))
            })?;
            generated(generate(name, &options.params).map_err(refused)?)?
        }
    };
    if report.date.is_empty() {
        report.date = date;
    }
    if !progress(0.2, "generated".to_owned()) {
        return Err(StoreError::Cancelled);
    }
    let template = templates
        .get(&options.template)
        .map_err(|e| refused(e.to_string()))?;
    // Named by its title, and by its template when it isn't OScope's.
    let name = match options.template.as_str() {
        STANDARD => report.title.clone(),
        other => format!("{} ({other})", report.title),
    };
    let mut draft = store.report(project, &name)?;
    let mut files = Vec::new();
    for format in &options.formats {
        let (file, bytes) = match format {
            Format::Pdf => (
                "report.pdf",
                pdf(&template.typst, &report, &images)
                    .map_err(|e| refused(e.to_string()))?
                    .bytes,
            ),
            Format::Html => (
                "report.html",
                html(&template.html, &report, &images)
                    .map_err(|e| refused(e.to_string()))?
                    .into_bytes(),
            ),
        };
        let path = draft.folder().join(file);
        std::fs::write(&path, bytes).map_err(|source| StoreError::Io { path, source })?;
        files.push(file.to_owned());
        if !progress(
            0.2 + 0.7 * files.len() as f64 / options.formats.len() as f64,
            format!("{file} made"),
        ) {
            return Err(StoreError::Cancelled);
        }
    }
    let path = draft.folder().join("model.json");
    let json = serde_json::to_vec_pretty(&report).map_err(|e| refused(e.to_string()))?;
    std::fs::write(&path, json).map_err(|source| StoreError::Io { path, source })?;
    files.push("model.json".to_owned());
    draft.record().files = files;
    draft.commit()
}

/// The job's parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    #[serde(default)]
    options: ReportOptions,
}

/// Makes report jobs, whose parameters are `project` and `options`, through
/// the templates in `templates`, by OScope's generator or one of
/// `generators`.
pub fn report_jobs(
    store: Arc<Store>,
    templates: Templates,
    generators: Option<Generators>,
) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: MAKE.to_owned(),
            title: "make a report".to_owned(),
            account: None,
            params: value.clone(),
            resource: Resource::Compute,
            pausable: false,
            gpu_mib: 0,
        };
        let (store, templates, generators) =
            (Arc::clone(&store), templates.clone(), generators.clone());
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(store, templates, generators, params, ctx))),
        })
    })
}

async fn run(
    store: Arc<Store>,
    templates: Templates,
    generators: Option<Generators>,
    params: Params,
    ctx: JobCtx,
) -> Result<(), Stop> {
    let job = ctx.clone();
    let project = params.project;
    let work = tokio::task::spawn_blocking(move || {
        let progress = |share: f64, what: String| {
            job.progress(Some(share), what);
            !job.is_cancelled()
        };
        make(
            &store,
            project,
            &params.options,
            &templates,
            generators.as_ref(),
            &progress,
        )
    });
    match work.await {
        Ok(Ok(report)) => {
            ctx.progress(Some(1.0), "done");
            ctx.add_result(format!("projects/{project}/reports/{}", report.id));
            Ok(())
        }
        Ok(Err(StoreError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}
