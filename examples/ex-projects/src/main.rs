//! `ex-projects`: projects on disk (P28). It makes two projects, each with an
//! acquisition, a field of the simulated H&E slide, and one with a result
//! made from it; then it serves a Gallery-style page of their cards. Its
//! README says how to open the stores in napari or Python.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_core::{Point2, Um, Xyz};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::{Field, HeTissue, Light, Optics, Slide, World};
use oscope_store::{
    FrameRecord, ImageInfo, ImageReader, Input, NewProject, PixelSize, Provenance, Query,
    SampleType, Samples, Store,
};
use serde_json::json;

/// The example's port, one of the manual checks'.
const PORT: u16 = 8233;
/// Where the slide's corner sits on the stage, and the stage z at which its
/// surface is in focus; the section's middle is 2 µm above it.
const SLIDE_AT: Point2 = Point2::new(10_000.0, 20_000.0);
const FOCUS: Um = Um(5_000.0);
const SECTION: f64 = 2.0;

/// Makes a project of a field of the simulated slide of `seed`, through
/// `objective`, `width` pixels wide; with a result, its luminance, if asked.
fn make(store: &Store, seed: u64, objective: &str, width: u32, result: bool) -> anyhow::Result<()> {
    let height = width * 3 / 4;
    let tissue = HeTissue::new(seed, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let world = World::with_slide(Slide::new(Arc::new(tissue), SLIDE_AT), FOCUS);
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let lens = catalogue.get(objective)?;
    let optics = Optics::new(&catalogue, lens, Um(2.4));
    let centre = Point2::new(SLIDE_AT.x + island.x, SLIDE_AT.y + island.y);
    let z = Um(FOCUS.0 + SECTION);
    let field = Field {
        centre,
        z,
        width,
        height,
    };
    let image = world.render(&field, &optics, Light::Transmitted);

    let project = store.create(NewProject {
        name: format!("H&E section {seed}"),
        sample: format!("Simulated H&E tissue, seed {seed}"),
        tags: vec![
            "he".to_owned(),
            "simulated".to_owned(),
            objective.to_owned(),
        ],
        owner: "ex-projects".to_owned(),
        ..NewProject::default()
    })?;
    let pixel = optics.pixel_size().0;
    let mut info = ImageInfo::new(u64::from(width), u64::from(height), 3, SampleType::U16);
    info.pixel_size = Some(PixelSize {
        x: pixel,
        y: pixel,
        z: 1.0,
    });
    info.channel_names = ["red", "green", "blue"].map(str::to_owned).to_vec();
    let mut draft = store.acquisition(project.id, &format!("Island 1, {objective}"), &info)?;
    let samples = image
        .data()
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 65_535.0).round() as u16);
    let size = (u64::from(width), u64::from(height));
    draft
        .image()
        .context("an image")?
        .write(0, (0, 0), size, &Samples::U16(samples.collect()))?;
    let record = draft.record();
    record.instrument = json!({"model": "OScope's simulated microscope"});
    record.objective = json!({"name": lens.name, "na": lens.na});
    record.camera = json!({"model": "simulated MIChrome 20", "pixel_um": 2.4});
    let position = Xyz::new(Um(centre.x), Um(centre.y), z);
    record.frames = vec![FrameRecord {
        index: 0,
        position: Some(position),
        timestamp: None,
        trigger: None,
    }];
    let acquisition = draft.commit()?;
    println!(
        "ex-projects: made {:?}, with {} of {width} × {height} pixels of {pixel:.3} µm in {} levels",
        project.name, acquisition.name, acquisition.image.levels
    );
    if !result {
        return Ok(());
    }
    // Its luminance, as a result: linear light, Rec. 709's weights.
    let weights = [0.2126_f32, 0.7152, 0.0722];
    let grey: Vec<f32> = image
        .data()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|rgb| rgb.iter().zip(weights).map(|(v, w)| v * w).sum())
        .collect();
    let provenance = Provenance {
        recipe: json!({"name": "luminance", "weights": weights}),
        nodes: Vec::new(),
        inputs: vec![Input::Acquisition(acquisition.id)],
        software: format!("ex-projects {}", env!("CARGO_PKG_VERSION")),
    };
    let grey_info = ImageInfo {
        channels: 1,
        sample: SampleType::F32,
        channel_names: Vec::new(),
        ..info
    };
    let draft = store.result(project.id, "Luminance", provenance, Some(&grey_info))?;
    draft
        .image()
        .context("an image")?
        .write(0, (0, 0), size, &Samples::F32(grey))?;
    draft.commit()?;
    Ok(())
}

/// The smoke checks: each project listed with its thumbnail and contents,
/// and each acquisition's store read back.
async fn check(address: SocketAddr, data: &Path) -> anyhow::Result<()> {
    let page = get(address, "/api/v1/projects/list").await?.json()?;
    ensure!(page["total"] == 2, "two projects: {page}");
    for project in page["projects"].as_array().context("projects")? {
        let id = project["id"].as_str().context("an ID")?;
        let thumbnail = get(address, &format!("/api/v1/projects/thumbnail/{id}")).await?;
        ensure!(
            thumbnail.status == 200 && thumbnail.content_type == "image/jpeg",
            "{id}'s thumbnail"
        );
        let contents = get(address, &format!("/api/v1/projects/project/{id}"))
            .await?
            .json()?;
        let acquisition = &contents["acquisitions"][0];
        let folder = data.join("projects").join(id).join("acquisitions");
        let store = folder
            .join(acquisition["id"].as_str().context("an acquisition")?)
            .join("raw.ome.zarr");
        let reader = ImageReader::open(&store)?;
        ensure!(
            reader.info().levels == acquisition["image"]["levels"],
            "{contents}"
        );
        let group: serde_json::Value =
            serde_json::from_slice(&std::fs::read(store.join("zarr.json"))?)?;
        ensure!(group["attributes"]["ome"]["version"] == "0.5", "NGFF 0.5");
        println!(
            "smoke: {} is listed, with its thumbnail and its store",
            project["name"]
        );
    }
    let found = get(address, "/api/v1/projects/list?text=seed%202")
        .await?
        .json()?;
    ensure!(found["total"] == 1, "a search: {found}");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-projects", PORT, &["--width"])?;
    let width = example
        .number("--width")
        .unwrap_or(if example.smoke { 640 } else { 2_048 }) as u32;
    let data = example.roots.data.clone();
    let store = Store::open(&data, &data.join("index.sqlite"))?;
    let existing = store.list(&Query::default())?.total;
    if existing == 0 {
        make(&store, 1, "10x", width, true)?;
        make(&store, 2, "20x", width, false)?;
    } else {
        println!(
            "ex-projects: {existing} projects already in {}",
            store.root().display()
        );
    }
    let api = Api::builder(&example.roots).projects(store);
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("page");
    example
        .serve(api, page, move |address| async move {
            check(address, &data).await
        })
        .await
}
