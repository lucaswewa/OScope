//! Analyses in the Workspace (P48, plan §5.4): a recipe run on a stored
//! image, or on a region of it, as a job, its outputs kept as a result of
//! the project with provenance enough to make it again:
//!
//! - **the recipe** as it ran, with the image and the region it read;
//! - **the nodes,** each with its version, and the extension that provides
//!   it, `id@version`, if one does;
//! - **a hash of what it read:** SHA-256 over the region's samples, as the
//!   recipe got them.
//!
//! A result made so can be made again, its nodes' parameters changed, and
//! two results' provenance compared.
//!
//! **Its outputs** are files in the result's folder:
//! - the first image is the result's own image, and later ones
//!   `<name>.ome.zarr`;
//! - masks are `<name>.png`, tables `<name>.csv`, shapes `<name>.geojson`
//!   and other JSON `<name>.json`;
//! - `summary.json` says what it read and made.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use oscope_compute::{Image, Shape};
use oscope_core::{ProjectId, ResultId};
use oscope_imaging::color::encode_png;
use oscope_jobs::Factory;
use oscope_pipeline::{
    Batch, Engine, Plan, PortType, Recipe, Run, RunError, RunOptions, Table, Value as Output,
    Values,
};
use oscope_store::{
    ImageInfo, ImageWriter, Input, NodeVersion, PixelSize, Provenance, RecordKind, ResultRecord,
    SampleType, Samples, Store, StoreError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::stain::Colour;

/// The job's kind.
pub const RUN: &str = "workspace.run";

/// The most pixels a run reads: a larger image needs a region.
const MOST: u64 = 25_000_000;

/// The names a result's folder holds already, which outputs can't take.
const RESERVED: [&str; 5] = ["image", "summary", "result", "annotations", "thumbnail"];

/// A region of an image, in µm from its top-left corner, x across and y
/// down, as annotations are (P44): on an image of unknown pixel size, a µm
/// is a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Roi {
    /// Its left edge.
    pub x: f64,
    /// Its top edge.
    pub y: f64,
    /// Its width.
    pub width: f64,
    /// Its height.
    pub height: f64,
}

/// An analysis: a recipe, and the image it runs on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    /// The recipe (ADR-0017). It takes `image`: grey or RGB in linear
    /// light from 0 to 1, a raw image's 2 × 2 cells in colour.
    pub recipe: Recipe,
    /// The image: an acquisition's or a result's.
    pub input: Input,
    /// A region of it: all of it unless given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roi: Option<Roi>,
    /// Tiles this many pixels square, to run tile by tile: whole unless
    /// given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile: Option<u32>,
    /// The result's name: the recipe's unless given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The extensions that provide nodes, `id@version`, by the nodes' IDs.
pub type Origins = BTreeMap<String, String>;

/// A difference between two results' provenance: where, as a JSON Pointer
/// (RFC 6901), and the two values, `null` where one has none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Change {
    /// Where.
    pub path: String,
    /// The first result's value.
    pub before: Value,
    /// The second's.
    pub after: Value,
}

/// What a run read.
struct Read {
    inputs: Values,
    hash: String,
    /// The pixels read: left, top, width and height.
    pixels: (u64, u64, u64, u64),
    /// A pixel's size as the recipe sees it, if the image's is known.
    pixel: Option<PixelSize>,
}

fn refused(why: impl Into<String>) -> StoreError {
    StoreError::Refused(why.into())
}

fn input_id(input: Input) -> (RecordKind, String) {
    match input {
        Input::Acquisition(a) => (RecordKind::Acquisition, a.to_string()),
        Input::Result(r) => (RecordKind::Result, r.to_string()),
    }
}

/// `analysis`'s recipe planned on `engine`'s nodes: refused unless it
/// takes one input, an image as `image`, and gives what a result keeps.
fn planned(engine: &Engine, analysis: &Analysis) -> Result<Plan, StoreError> {
    let plan = engine
        .plan(&analysis.recipe)
        .map_err(|e| refused(e.to_string()))?;
    let inputs = &analysis.recipe.inputs;
    if inputs.len() != 1 || inputs[0].name != "image" || inputs[0].kind != PortType::Image {
        return Err(refused(
            "a recipe in the Workspace takes one input, the image `image`",
        ));
    }
    let first = plan
        .outputs()
        .into_iter()
        .find(|(_, k)| *k == PortType::Image);
    for (name, kind) in plan.outputs() {
        if matches!(kind, PortType::Stack | PortType::Report) {
            return Err(refused(format!(
                "the output {name} is a {kind:?}: a result keeps images, masks, tables, shapes and JSON"
            )));
        }
        if RESERVED.contains(&name) && Some((name, kind)) != first {
            return Err(refused(format!(
                "an output can't be named {name}: the result's folder has one"
            )));
        }
    }
    Ok(plan)
}

/// Reads `analysis`'s region of its image, as its recipe takes it.
fn read(store: &Store, project: ProjectId, analysis: &Analysis) -> Result<Read, StoreError> {
    let (kind, id) = input_id(analysis.input);
    let (colour, info) = Colour::open(store, project, kind, &id)?;
    // A pixel the recipe sees, in µm: a raw image's cell is two of its own.
    let cell = colour.cell as f64;
    let (sx, sy) = info.pixel_size.map_or((1.0, 1.0), |p| (p.x, p.y));
    let (px, py) = (sx * cell, sy * cell);
    let (width, height) = colour.size;
    let pixels = match analysis.roi {
        None => (0, 0, width, height),
        Some(roi) => {
            let finite = [roi.x, roi.y, roi.width, roi.height]
                .iter()
                .all(|v| v.is_finite());
            if !finite || roi.width <= 0.0 || roi.height <= 0.0 {
                return Err(refused(format!(
                    "a region is finite, and wider and higher than 0: {roi:?}"
                )));
            }
            let at = |v: f64, size: f64, most: u64| (v / size).clamp(0.0, most as f64);
            let (x0, y0) = (at(roi.x, px, width).floor(), at(roi.y, py, height).floor());
            let x1 = at(roi.x + roi.width, px, width).ceil();
            let y1 = at(roi.y + roi.height, py, height).ceil();
            if x1 <= x0 || y1 <= y0 {
                return Err(refused(format!(
                    "the region {roi:?} is outside the image, of {:.1} × {:.1} µm",
                    width as f64 * px,
                    height as f64 * py
                )));
            }
            (x0 as u64, y0 as u64, (x1 - x0) as u64, (y1 - y0) as u64)
        }
    };
    let (_, _, w, h) = pixels;
    if w * h > MOST {
        return Err(refused(format!(
            "{w} × {h} pixels: a run reads {MOST} at most, so give it a region"
        )));
    }
    let (samples, channels) = colour.samples(pixels)?;
    let mut hasher = Sha256::new();
    for chunk in samples.chunks(1 << 14) {
        let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
        hasher.update(&bytes);
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let image = Image::from_vec(Shape::new(w as u32, h as u32, channels), samples)
        .map_err(|e| refused(e.to_string()))?;
    Ok(Read {
        inputs: Values::from([("image".to_owned(), Output::image(image))]),
        hash: format!("sha256:{hex}"),
        pixels,
        pixel: info.pixel_size.map(|p| PixelSize {
            x: px,
            y: py,
            z: p.z,
        }),
    })
}

/// `analysis`'s provenance, as it ran on `engine`, with what it read.
fn provenance(engine: &Engine, origins: &Origins, analysis: &Analysis, read: &Read) -> Provenance {
    let mut nodes: Vec<NodeVersion> = analysis
        .recipe
        .nodes
        .iter()
        .map(|used| NodeVersion {
            node: used.node.clone(),
            version: engine
                .registry()
                .get(&used.node)
                .map_or_else(String::new, |n| n.descriptor().version.to_string()),
            extension: origins.get(&used.node).cloned(),
        })
        .collect();
    nodes.sort_by(|a, b| a.node.cmp(&b.node));
    nodes.dedup();
    let (_, id) = input_id(analysis.input);
    Provenance {
        recipe: json!({ RUN: analysis }),
        nodes,
        inputs: vec![analysis.input],
        hashes: BTreeMap::from([(id, read.hash.clone())]),
        software: format!("OScope {}", env!("CARGO_PKG_VERSION")),
    }
}

/// `table` as CSV (RFC 4180), its JSON's text as it is and null empty.
fn csv(table: &Table) -> String {
    let field = |text: String| match text.contains([',', '"', '\r', '\n']) {
        true => format!("\"{}\"", text.replace('"', "\"\"")),
        false => text,
    };
    let cell = |value: &Value| match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let mut out = String::new();
    let header: Vec<String> = table.columns.iter().cloned().map(field).collect();
    out += &(header.join(",") + "\r\n");
    for row in &table.rows {
        let cells: Vec<String> = row.iter().map(|v| field(cell(v))).collect();
        out += &(cells.join(",") + "\r\n");
    }
    out
}

/// Writes output `name`, not the result's own image, in `folder`: its
/// file's name.
fn output(
    folder: &Path,
    name: &str,
    value: &Output,
    pixel: Option<PixelSize>,
) -> Result<String, StoreError> {
    let save = |file: String, bytes: Vec<u8>| {
        let path = folder.join(&file);
        std::fs::write(&path, bytes).map_err(|source| StoreError::Io { path, source })?;
        Ok(file)
    };
    let json = |value: &Value| serde_json::to_vec_pretty(value).map_err(|e| refused(e.to_string()));
    let missing = || refused(format!("the output {name} isn't in memory"));
    match value.port_type() {
        PortType::Image => {
            let image = value.as_image().ok_or_else(missing)?;
            let file = format!("{name}.ome.zarr");
            let (w, h) = (u64::from(image.width()), u64::from(image.height()));
            let mut info = ImageInfo::new(w, h, image.channels() as u32, SampleType::F32);
            info.pixel_size = pixel;
            let writer = ImageWriter::create(&folder.join(&file), &info, name)?;
            writer.write(0, (0, 0), (w, h), &Samples::F32(image.data().to_vec()))?;
            writer.finish()?;
            Ok(file)
        }
        PortType::Mask => {
            let mask = value.as_mask().ok_or_else(missing)?;
            let rgb: Vec<u8> = mask
                .data()
                .iter()
                .flat_map(|&m| [if m > 0 { 255 } else { 0 }; 3])
                .collect();
            let mut png = Vec::new();
            encode_png(&mut png, &rgb, mask.width(), mask.height())
                .map_err(|e| refused(e.to_string()))?;
            save(format!("{name}.png"), png)
        }
        PortType::Table => {
            let table = value.as_table().ok_or_else(missing)?;
            save(format!("{name}.csv"), csv(table).into_bytes())
        }
        PortType::Geometry => save(
            format!("{name}.geojson"),
            json(value.as_json().ok_or_else(missing)?)?,
        ),
        _ => save(
            format!("{name}.json"),
            json(value.as_json().ok_or_else(missing)?)?,
        ),
    }
}

/// Keeps `run`'s outputs as a result of `project`.
fn write(
    store: &Store,
    project: ProjectId,
    engine: &Engine,
    origins: &Origins,
    analysis: &Analysis,
    read: &Read,
    run: Run,
) -> Result<ResultRecord, StoreError> {
    let names: Vec<&str> = analysis
        .recipe
        .outputs
        .iter()
        .map(|o| o.name.as_str())
        .collect();
    let got = |name: &str| {
        run.outputs
            .get(name)
            .ok_or_else(|| refused(format!("the run gave no {name}")))
    };
    // The first image is the result's own.
    let first = names
        .iter()
        .copied()
        .find(|n| run.outputs.get(*n).and_then(Output::as_image).is_some());
    let own = first.and_then(|n| run.outputs[n].as_image());
    let info = own.map(|image| {
        let (w, h) = (u64::from(image.width()), u64::from(image.height()));
        let mut info = ImageInfo::new(w, h, image.channels() as u32, SampleType::F32);
        info.pixel_size = read.pixel;
        info
    });
    let name = match (&analysis.name, analysis.recipe.name.as_str()) {
        (Some(name), _) => name.clone(),
        (None, "") => "an analysis".to_owned(),
        (None, recipe) => recipe.to_owned(),
    };
    let made = provenance(engine, origins, analysis, read);
    let draft = store.result(project, &name, made, info.as_ref())?;
    let mut files = Map::new();
    let mut rows = Map::new();
    for name in &names {
        let value = got(name)?;
        let file = match (Some(*name) == first, own, draft.image()) {
            (true, Some(image), Some(writer)) => {
                let size = (u64::from(image.width()), u64::from(image.height()));
                writer.write(0, (0, 0), size, &Samples::F32(image.data().to_vec()))?;
                "image.ome.zarr".to_owned()
            }
            _ => output(draft.folder(), name, value, read.pixel)?,
        };
        if let Some(table) = value.as_table() {
            rows.insert((*name).to_owned(), json!(table.rows.len()));
        }
        files.insert((*name).to_owned(), json!(file));
    }
    let (x, y, w, h) = read.pixels;
    let summary = json!({
        "recipe": analysis.recipe.name,
        "input": input_id(analysis.input).1,
        "pixels": [x, y, w, h],
        "roi_um": analysis.roi,
        "outputs": files,
        "rows": rows,
    });
    let path = draft.folder().join("summary.json");
    let text = serde_json::to_vec_pretty(&summary).map_err(|e| refused(e.to_string()))?;
    std::fs::write(&path, text).map_err(|source| StoreError::Io { path, source })?;
    draft.commit()
}

/// Runs `analysis` on its image of `project` now, on `engine`, as `options`
/// say: the result it keeps. `origins` names the extensions that provide
/// nodes.
pub fn analyse(
    store: &Store,
    project: ProjectId,
    engine: &Engine,
    origins: &Origins,
    analysis: &Analysis,
    options: &RunOptions,
) -> Result<ResultRecord, StoreError> {
    let plan = planned(engine, analysis)?;
    let read = read(store, project, analysis)?;
    let run = match analysis.tile {
        Some(size) => engine.run_tiled(&plan, &read.inputs, size, options, |_, _| {}),
        None => engine.run(&plan, &read.inputs, options),
    };
    let run = run.map_err(|e| match e {
        RunError::Cancelled => StoreError::Cancelled,
        e => refused(e.to_string()),
    })?;
    write(store, project, engine, origins, analysis, &read, run)
}

/// The analysis that made result `id` of `project`, its nodes' parameters
/// changed as `changes` says: by the node's ID in the recipe, each
/// parameter's new value, or `null` for its default.
pub fn again(
    store: &Store,
    project: ProjectId,
    id: ResultId,
    changes: &BTreeMap<String, Map<String, Value>>,
) -> Result<Analysis, StoreError> {
    let contents = store.contents(project)?;
    let result = contents
        .results
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| StoreError::NotFound {
            kind: "result",
            id: id.to_string(),
        })?;
    let made = result.provenance.recipe.get(RUN).cloned().ok_or_else(|| {
        refused(format!(
            "result {id} wasn't made in the Workspace: it has no analysis to run again"
        ))
    })?;
    let mut analysis: Analysis = serde_json::from_value(made)
        .map_err(|e| refused(format!("result {id}'s analysis: {e}")))?;
    for (node, params) in changes {
        let used = analysis
            .recipe
            .nodes
            .iter_mut()
            .find(|n| &n.id == node)
            .ok_or_else(|| refused(format!("the recipe has no node {node}")))?;
        if !used.params.is_object() {
            used.params = json!({});
        }
        let current = used.params.as_object_mut().expect("an object");
        for (key, value) in params {
            match value.is_null() {
                true => current.remove(key),
                false => current.insert(key.clone(), value.clone()),
            };
        }
    }
    Ok(analysis)
}

/// `value` with each list whose items each have a distinct `id`, `node` or
/// `name` made an object by it, so a recipe's nodes are compared by their
/// IDs and not their places.
fn keyed(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            let key = |item: &Value| {
                ["id", "node", "name"]
                    .iter()
                    .find_map(|k| item.get(*k).and_then(Value::as_str))
                    .map(str::to_owned)
            };
            let keys: Option<Vec<String>> = items.iter().map(key).collect();
            match keys {
                Some(keys)
                    if !keys.is_empty()
                        && keys.iter().collect::<BTreeSet<_>>().len() == keys.len() =>
                {
                    Value::Object(keys.into_iter().zip(items.into_iter().map(keyed)).collect())
                }
                _ => Value::Array(items.into_iter().map(keyed).collect()),
            }
        }
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, keyed(v))).collect()),
        other => other,
    }
}

/// `value`'s leaves, by their JSON Pointers: an empty object or list has
/// none, as a node's parameters left out are the same as none.
fn leaves(path: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                leaves(&format!("{path}/{key}"), inner, out);
            }
        }
        Value::Array(items) => {
            for (i, inner) in items.iter().enumerate() {
                leaves(&format!("{path}/{i}"), inner, out);
            }
        }
        other => {
            out.insert(path.to_owned(), other.clone());
        }
    }
}

/// What differs between `a` and `b`, two results' provenance. A recipe's
/// nodes go by their IDs, and the nodes' versions by the nodes'.
pub fn difference(a: &Provenance, b: &Provenance) -> Vec<Change> {
    let flat = |p: &Provenance| {
        let mut out = BTreeMap::new();
        leaves(
            "",
            &keyed(serde_json::to_value(p).unwrap_or_default()),
            &mut out,
        );
        out
    };
    let (before, after) = (flat(a), flat(b));
    let paths: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    paths
        .into_iter()
        .filter(|p| before.get(*p) != after.get(*p))
        .map(|p| Change {
            path: p.clone(),
            before: before.get(p).cloned().unwrap_or(Value::Null),
            after: after.get(p).cloned().unwrap_or(Value::Null),
        })
        .collect()
}

/// The job's parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    analysis: Analysis,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Makes `workspace.run` jobs, whose parameters are `project` and
/// `analysis`: batch runs of one item on `engine` (ADR-0017), each keeping
/// a result in `store`. `origins` names the extensions that provide nodes.
pub fn workspace_jobs(store: Arc<Store>, engine: Arc<Engine>, origins: Arc<Origins>) -> Factory {
    Arc::new(move |value| {
        let Params { project, analysis } = Params::deserialize(value).map_err(|e| e.to_string())?;
        let plan = planned(&engine, &analysis).map_err(|e| e.to_string())?;
        let kept: Arc<Mutex<Option<Read>>> = Arc::default();
        let (from, into) = (Arc::clone(&store), Arc::clone(&kept));
        let wanted = analysis.clone();
        let reading = Box::new(move |_: usize| {
            let read = read(&from, project, &wanted).map_err(|e| e.to_string())?;
            let inputs = read.inputs.clone();
            *lock(&into) = Some(read);
            Ok(inputs)
        });
        let (into, on, named) = (
            Arc::clone(&store),
            Arc::clone(&engine),
            Arc::clone(&origins),
        );
        let ran = analysis.clone();
        let writing = Box::new(move |_: usize, run: Run| {
            let read = lock(&kept).take().ok_or("nothing was read")?;
            let made = write(&into, project, &on, &named, &ran, &read, run);
            let made = made.map_err(|e| e.to_string())?;
            Ok(Some(format!("projects/{project}/results/{}", made.id)))
        });
        let batch = Batch {
            count: 1,
            read: reading,
            write: writing,
            tile: analysis.tile,
        };
        let title = format!("analyse with {}", analysis.recipe.name);
        let mut job = engine.batch_job(&title, Arc::new(plan), batch, 0);
        job.spec.kind = RUN.to_owned();
        job.spec.params = value.clone();
        job.spec.pausable = false;
        Ok(job)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_written_as_rfc_4180_has_them() {
        let table = Table {
            columns: vec!["name".to_owned(), "a, b".to_owned()],
            rows: vec![
                vec![json!("say \"hi\""), json!(1.5)],
                vec![json!("plain"), Value::Null],
            ],
        };
        let want = "name,\"a, b\"\r\n\"say \"\"hi\"\"\",1.5\r\nplain,\r\n";
        assert_eq!(csv(&table), want);
    }
}
