//! A node run once on the live view's newest frame (W8.5): what an
//! extension's panel submits (ADR-0036). The frame is in linear light, as
//! live recipes see it, and goes to each of the node's image inputs; each
//! output is answered as JSON, an image as a PNG no longer than 1024 on its
//! longest side.

use std::sync::Arc;
use std::time::Duration;

use oscope_acquire::preview::Preview;
use oscope_compute::{Image, Shape};
use oscope_imaging::color::encode_png;
use oscope_pipeline::{Engine, Plan, PortType, Recipe, RunOptions, Value, Values};
use oscope_report::to_base64;
use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::devices::Devices;

/// What is run: the node, and its parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunNode {
    /// The node, by its ID.
    pub node: String,
    /// Its parameters, as its schema has them.
    #[serde(default)]
    pub params: Json,
}

/// How long a run waits for a frame: the camera must be streaming.
const WAIT: Duration = Duration::from_secs(10);

/// The longest side of an image answered.
const SHOWN: u32 = 1024;

/// Why a run failed: 400 for what was asked, 503 for the camera.
pub(crate) enum Failed {
    Refused(String),
    NoFrame,
    Run(String),
}

/// `asked` run once on `devices`' newest frame through `preview`: its
/// outputs, by port.
pub(crate) async fn run(
    engine: Arc<Engine>,
    preview: Preview,
    devices: Arc<Devices>,
    asked: RunNode,
) -> Result<Json, Failed> {
    let recipe = recipe_of(&engine, &asked)?;
    let plan =
        Plan::new(engine.registry(), &recipe, false).map_err(|e| Failed::Refused(e.to_string()))?;
    let mut latest = devices.acquisition.bus().subscribe_latest("run node");
    let frame = tokio::time::timeout(WAIT, latest.recv()).await;
    let frame = frame.ok().flatten().ok_or(Failed::NoFrame)?;
    tokio::task::spawn_blocking(move || {
        let image = preview
            .linear(&frame)
            .map_err(|e| Failed::Run(e.to_string()))?;
        let inputs = Values::from([("image".to_owned(), Value::image(image))]);
        let options = RunOptions {
            memoise: false,
            ..RunOptions::default()
        };
        let ran = engine.run(&plan, &inputs, &options);
        let ran = ran.map_err(|e| Failed::Run(e.to_string()))?;
        let answer = ran
            .outputs
            .iter()
            .map(|(port, value)| (port.clone(), shown(value)));
        Ok(Json::Object(answer.collect()))
    })
    .await
    .map_err(|e| Failed::Run(e.to_string()))?
}

/// A recipe of `asked`'s node alone: the frame to each of its image
/// inputs, and each of its outputs the recipe's own.
fn recipe_of(engine: &Engine, asked: &RunNode) -> Result<Recipe, Failed> {
    let node = engine.registry().get(&asked.node);
    let node = node.ok_or_else(|| Failed::Refused(format!("no node is {:?}", asked.node)))?;
    let described = node.descriptor();
    let images = described
        .inputs
        .iter()
        .filter(|p| p.kind == PortType::Image);
    let edges: Vec<Json> = images
        .map(|port| json!({"from": "input.image", "to": format!("node.{}", port.name)}))
        .collect();
    if edges.is_empty() {
        let said = format!("{} takes no image to run on", asked.node);
        return Err(Failed::Refused(said));
    }
    let outputs: Vec<Json> = (described.outputs.iter())
        .map(|port| json!({"name": port.name, "from": format!("node.{}", port.name)}))
        .collect();
    let params = match &asked.params {
        Json::Null => json!({}),
        given => given.clone(),
    };
    let recipe = json!({
        "format": 1,
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [{"id": "node", "node": asked.node, "params": params}],
        "edges": edges,
        "outputs": outputs,
    });
    serde_json::from_value(recipe).map_err(|e| Failed::Refused(e.to_string()))
}

/// A value as the app shows it: an image as a PNG in base64, a table its
/// columns and rows, any JSON as it is, and what else there is said.
fn shown(value: &Value) -> Json {
    if let Some(image) = value.as_image() {
        let small = smaller(image, SHOWN);
        let (width, height) = (small.width(), small.height());
        return match png(&small) {
            Ok(png) => json!({"image": {"width": width, "height": height, "png": png}}),
            Err(error) => json!({"error": error}),
        };
    }
    if let Some(table) = value.as_table() {
        return json!({"table": {"columns": table.columns, "rows": table.rows}});
    }
    match value.as_json() {
        Some(json) => json.clone(),
        None => json!({"error": "a value the panel can't show"}),
    }
}

/// `image`, its RGB from 0 to 1, as a PNG in base64.
fn png(image: &Image) -> Result<String, String> {
    let channels = image.channels();
    let byte = |sample: f32| (sample.clamp(0.0, 1.0) * 255.0).round() as u8;
    let rgb: Vec<u8> = (image.data().chunks(channels))
        .flat_map(|p| [0, 1, 2].map(|c| byte(p[c.min(channels - 1)])))
        .collect();
    let mut file = Vec::new();
    encode_png(&mut file, &rgb, image.width(), image.height()).map_err(|e| e.to_string())?;
    Ok(to_base64(&file))
}

/// `image` in blocks, each its pixels' mean, so that neither side is longer
/// than `most`; as it is, if neither is.
fn smaller(image: &Image, most: u32) -> Image {
    let (width, height, channels) = (image.width(), image.height(), image.channels());
    let block = width.max(height).div_ceil(most.max(1)).max(1);
    if block == 1 {
        return image.clone();
    }
    let (across, down) = (width.div_ceil(block), height.div_ceil(block));
    let mut data = Vec::with_capacity((across * down) as usize * channels);
    for by in 0..down {
        for bx in 0..across {
            let xs = bx * block..((bx + 1) * block).min(width);
            let ys = by * block..((by + 1) * block).min(height);
            let n = (xs.len() * ys.len()) as f32;
            for c in 0..channels {
                let mut sum = 0.0;
                for y in ys.clone() {
                    for x in xs.clone() {
                        sum += image.data()[(y * width + x) as usize * channels + c];
                    }
                }
                data.push(sum / n);
            }
        }
    }
    Image::from_vec(Shape::new(across, down, channels), data).expect("the blocks fill the image")
}
