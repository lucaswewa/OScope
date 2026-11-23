//! Reports as a job (P47): OScope's generator's report on a mosaic and its
//! count of cells, as PDF, HTML and JSON; one result chosen; a template of
//! the user's; an extension's report, its image in base64; refusals; a
//! cancel.

use std::sync::Arc;

use oscope_core::ProjectId;
use oscope_report::{Block, Report, STANDARD, Templates};
use oscope_store::{
    ImageInfo, Input, NewProject, NodeVersion, PixelSize, Provenance, SampleType, Samples, Store,
    StoreError,
};
use oscope_workflows::report::{Format, Generators, ReportOptions, make};
use serde_json::{Value, json};

struct Made {
    store: Store,
    project: ProjectId,
    mosaic: String,
    cells: String,
    templates: Templates,
    _folder: tempfile::TempDir,
}

fn provenance(inputs: Vec<Input>, node: &str) -> Provenance {
    Provenance {
        recipe: Value::Null,
        nodes: vec![NodeVersion {
            node: node.to_owned(),
            version: "0.1.0".to_owned(),
            extension: None,
        }],
        inputs,
        hashes: Default::default(),
        software: "OScope 0.1.0".to_owned(),
    }
}

/// A project with a mosaic, an image, and its cells counted: a result with
/// `cells.csv` and `summary.json`.
fn made() -> Made {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "slides".to_owned(),
        sample: "an H&E slide".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let mut info = ImageInfo::new(64, 48, 3, SampleType::U8);
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.5,
        z: 1.0,
    });
    let draft = store
        .result(
            project,
            "a mosaic",
            provenance(Vec::new(), "stitch.run"),
            Some(&info),
        )
        .unwrap();
    let pixels: Vec<u8> = (0..64 * 48 * 3).map(|i| (i % 251) as u8).collect();
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (64, 48), &Samples::U8(pixels))
        .unwrap();
    let mosaic = draft.commit().unwrap().id;
    let inputs = vec![Input::Result(mosaic)];
    let draft = store
        .result(
            project,
            "a mosaic: cells",
            provenance(inputs, "cells.count"),
            None,
        )
        .unwrap();
    let mut csv = String::from("id,x_um,y_um,area_um2,perimeter_um,circularity,mean\r\n");
    // Areas of 21 to 50 µm²: the largest on the last bin's edge.
    for k in 0..30 {
        csv += &format!("{},{},{},{},20.5,0.9,0.7\r\n", k + 1, k * 10, k * 5, 21 + k);
    }
    std::fs::write(draft.folder().join("cells.csv"), csv).unwrap();
    let summary = json!({"count": 30, "per_mm2": 4586.25, "dab": null, "white": [0.9, 0.85, 0.8]});
    std::fs::write(draft.folder().join("summary.json"), summary.to_string()).unwrap();
    let cells = draft.commit().unwrap().id;
    let templates = Templates::open(&folder.path().join("templates"));
    Made {
        store,
        project,
        mosaic: mosaic.to_string(),
        cells: cells.to_string(),
        templates,
        _folder: folder,
    }
}

fn dated() -> ReportOptions {
    ReportOptions {
        date: Some("2026-11-16".to_owned()),
        ..ReportOptions::default()
    }
}

#[test]
fn a_report_on_the_results_is_made_as_pdf_html_and_json() {
    let m = made();
    let report = make(
        &m.store,
        m.project,
        &dated(),
        &m.templates,
        None,
        &|_, _| true,
    )
    .unwrap();
    assert_eq!(report.files, ["report.pdf", "report.html", "model.json"]);
    assert_eq!(report.name, "slides: results");
    let folder = m
        .store
        .folder(m.project)
        .join(format!("reports/{}", report.id));
    let pdf = std::fs::read(folder.join("report.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let model: Report =
        serde_json::from_str(&std::fs::read_to_string(folder.join("model.json")).unwrap()).unwrap();
    assert_eq!(
        (model.title.as_str(), model.date.as_str()),
        ("slides: results", "2026-11-16")
    );
    let headings: Vec<&str> = model.sections.iter().map(|s| s.heading.as_str()).collect();
    assert_eq!(headings, ["a mosaic", "a mosaic: cells"]);
    // The mosaic: its thumbnail, and a region at full size, here all of it.
    let captions: Vec<&str> = model.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Image { caption, .. } => Some(caption.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        captions,
        [
            "a mosaic, whole",
            "A region of 32 × 24 µm at its middle, at full size"
        ]
    );
    // The count: its input's picture, its summary, its areas' chart and its
    // first rows.
    let kinds: Vec<&str> = model.sections[1]
        .blocks
        .iter()
        .map(|b| match b {
            Block::Image { .. } => "image",
            Block::Table { .. } => "table",
            Block::Chart { .. } => "chart",
            Block::Text { .. } => "text",
        })
        .collect();
    assert_eq!(kinds, ["image", "table", "chart", "table"]);
    let Block::Table { rows, .. } = &model.sections[1].blocks[1] else {
        panic!()
    };
    assert!(
        rows.contains(&vec!["per_mm2".to_owned(), "4586.25".to_owned()])
            && rows.contains(&vec!["white".to_owned(), "0.9, 0.85, 0.8".to_owned()])
            && rows.contains(&vec!["dab".to_owned(), "none".to_owned()])
    );
    let Block::Chart { chart, .. } = &model.sections[1].blocks[2] else {
        panic!()
    };
    assert_eq!(chart.values.iter().sum::<f64>(), 30.0);
    let Block::Table { rows, .. } = &model.sections[1].blocks[3] else {
        panic!()
    };
    assert_eq!(rows.len(), 10);
    let html = std::fs::read_to_string(folder.join("report.html")).unwrap();
    assert!(
        html.contains("a mosaic: cells")
            && html.contains("data:image/jpeg;base64,")
            && html.contains("<svg")
    );
    assert_eq!(
        model.provenance[1].value,
        "OScope 0.1.0, by cells.count 0.1.0"
    );
    // One result chosen, HTML alone.
    let one = ReportOptions {
        params: json!({"results": [m.cells]}),
        formats: vec![Format::Html],
        ..dated()
    };
    let report = make(&m.store, m.project, &one, &m.templates, None, &|_, _| true).unwrap();
    assert_eq!(report.files, ["report.html", "model.json"]);
    let folder = m
        .store
        .folder(m.project)
        .join(format!("reports/{}", report.id));
    let model: Report =
        serde_json::from_str(&std::fs::read_to_string(folder.join("model.json")).unwrap()).unwrap();
    assert_eq!(model.sections.len(), 1);
    // Through a template of the user's, which names it.
    let standard = m.templates.get(STANDARD).unwrap();
    m.templates
        .save("mine", &standard.typst, &standard.html)
        .unwrap();
    let theirs = ReportOptions {
        params: json!({"results": [m.mosaic]}),
        template: "mine".to_owned(),
        ..dated()
    };
    let report = make(&m.store, m.project, &theirs, &m.templates, None, &|_, _| {
        true
    })
    .unwrap();
    assert_eq!(report.name, "slides: results (mine)");
    // A grey image: its region too.
    let info = ImageInfo::new(40, 30, 1, SampleType::U16);
    let made = provenance(Vec::new(), "volume.deconvolve");
    let draft = m
        .store
        .result(m.project, "a grey image", made, Some(&info))
        .unwrap();
    let pixels: Vec<u16> = (0..40 * 30).map(|i| (i * 50) as u16).collect();
    let image = draft.image().unwrap();
    image
        .write(0, (0, 0), (40, 30), &Samples::U16(pixels))
        .unwrap();
    let grey = draft.commit().unwrap().id.to_string();
    let alone = ReportOptions {
        params: json!({"results": [grey]}),
        formats: vec![Format::Html],
        ..dated()
    };
    let report = make(&m.store, m.project, &alone, &m.templates, None, &|_, _| {
        true
    })
    .unwrap();
    let folder = m
        .store
        .folder(m.project)
        .join(format!("reports/{}", report.id));
    let model: Report =
        serde_json::from_str(&std::fs::read_to_string(folder.join("model.json")).unwrap()).unwrap();
    let captions: Vec<&str> = model.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Image { caption, .. } => Some(caption.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        captions,
        [
            "a grey image, whole",
            "A region of 40 × 30 pixels at its middle, at full size"
        ]
    );
}

#[test]
fn an_extension_s_report_is_rendered_and_what_isn_t_is_refused() {
    let m = made();
    // A 1 × 1 PNG, in base64.
    let dot = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAAAAABX3VL4AAAADklEQVR4nGNpYGBiYAAAAqQAhz/Ex4IAAAAASUVORK5CYII=";
    let generators: Generators = Arc::new(move |name, params| match name {
        "example.squares" => {
            let n = params["count"].as_u64().ok_or("`count`")?;
            let rows: Vec<Value> = (0..n)
                .map(|k| json!([k.to_string(), (k * k).to_string()]))
                .collect();
            Ok(json!({
                "title": "Squares", "date": "",
                "sections": [{"heading": "Squares", "blocks": [
                    {"kind": "table", "columns": ["k", "k²"], "rows": rows},
                    {"kind": "image", "name": "dot.png"},
                ]}],
                "images": {"dot.png": dot},
            }))
        }
        "example.broken" => {
            Ok(json!({"title": "Broken", "date": "", "images": {"dot.png": "not base64!"}}))
        }
        other => Err(format!("no report generator {other}")),
    });
    let options = ReportOptions {
        generator: "example.squares".to_owned(),
        params: json!({"count": 4}),
        ..dated()
    };
    let report = make(
        &m.store,
        m.project,
        &options,
        &m.templates,
        Some(&generators),
        &|_, _| true,
    )
    .unwrap();
    let folder = m
        .store
        .folder(m.project)
        .join(format!("reports/{}", report.id));
    let html = std::fs::read_to_string(folder.join("report.html")).unwrap();
    assert!(
        html.contains("<td>9</td>")
            && html.contains("2026-11-16")
            && html.contains(&format!("data:image/png;base64,{dot}"))
    );
    // Refused: what the generator refuses; images not in base64; a
    // generator without the extensions; a template that isn't.
    let mut wrong = options.clone();
    wrong.params = json!({});
    let refused = make(
        &m.store,
        m.project,
        &wrong,
        &m.templates,
        Some(&generators),
        &|_, _| true,
    );
    assert!(
        matches!(&refused, Err(StoreError::Refused(why)) if why.contains("`count`")),
        "{refused:?}"
    );
    let broken = ReportOptions {
        generator: "example.broken".to_owned(),
        ..dated()
    };
    assert!(matches!(
        make(
            &m.store,
            m.project,
            &broken,
            &m.templates,
            Some(&generators),
            &|_, _| true
        ),
        Err(StoreError::Refused(_))
    ));
    assert!(matches!(
        make(
            &m.store,
            m.project,
            &options,
            &m.templates,
            None,
            &|_, _| true
        ),
        Err(StoreError::Refused(_))
    ));
    let nameless = ReportOptions {
        template: "none".to_owned(),
        ..dated()
    };
    assert!(matches!(
        make(
            &m.store,
            m.project,
            &nameless,
            &m.templates,
            None,
            &|_, _| true
        ),
        Err(StoreError::Refused(_))
    ));
    let none = ReportOptions {
        params: json!({"results": ["none"]}),
        ..dated()
    };
    assert!(matches!(
        make(&m.store, m.project, &none, &m.templates, None, &|_, _| true),
        Err(StoreError::Refused(_))
    ));
    // A cancel keeps nothing, at its first chance, before rendering, or a
    // later one.
    let before = m.store.contents(m.project).unwrap().reports.len();
    let told = std::cell::RefCell::new(Vec::new());
    let stopped = make(
        &m.store,
        m.project,
        &dated(),
        &m.templates,
        None,
        &|_, what| {
            told.borrow_mut().push(what);
            false
        },
    );
    assert!(matches!(stopped, Err(StoreError::Cancelled)));
    assert_eq!(*told.borrow(), ["generated"]);
    let cancelled = make(
        &m.store,
        m.project,
        &dated(),
        &m.templates,
        None,
        &|share, _| share < 0.5,
    );
    assert!(matches!(cancelled, Err(StoreError::Cancelled)));
    assert_eq!(m.store.contents(m.project).unwrap().reports.len(), before);
}
