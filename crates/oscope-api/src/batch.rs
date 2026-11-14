//! The `pipeline.batch` job (Checkpoint B): a recipe over the camera's next
//! frames, each in linear light, as live recipes see them, and its image
//! outputs saved as PNG files in the data root.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use oscope_acquire::preview::Preview;
use oscope_imaging::color::encode_png;
use oscope_jobs::Factory;
use oscope_pipeline::{Batch, Engine, Plan, Recipe, Run, Value, Values};
use serde::Deserialize;
use serde_json::json;

use crate::devices::Devices;

/// The job's parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    /// The recipe: its input `image`, and `region` if it takes one, the
    /// whole frame.
    recipe: Recipe,
    /// How many of the camera's next frames, from 1 to 100.
    #[serde(default = "one")]
    frames: usize,
    /// Tiles' size, to run tile by tile.
    #[serde(default)]
    tile: Option<u32>,
}

fn one() -> usize {
    1
}

/// How long a batch waits for a frame: the camera must be streaming.
const WAIT: Duration = Duration::from_secs(10);

/// The factory of `pipeline.batch` jobs, running on `engine`, taking frames
/// from `devices` through `preview`, and saving in `folder`.
pub(crate) fn factory(
    engine: Arc<Engine>,
    preview: Preview,
    devices: Arc<Devices>,
    folder: PathBuf,
) -> Factory {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    Arc::new(move |params| {
        let Params {
            recipe,
            frames,
            tile,
        } = Params::deserialize(params).map_err(|e| e.to_string())?;
        if !(1..=100).contains(&frames) {
            return Err(format!("frames: {frames}, where 1 to 100 may be"));
        }
        let plan = Plan::new(engine.registry(), &recipe, false).map_err(|e| e.to_string())?;
        let region = recipe.inputs.iter().any(|input| input.name == "region");
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let name = format!("{stamp}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let out = folder.join(&name);
        let handle = tokio::runtime::Handle::current();
        let mut latest = devices.acquisition.bus().subscribe_latest("batch");
        let preview = preview.clone();
        let read = Box::new(move |_| {
            let frame = handle.block_on(async { tokio::time::timeout(WAIT, latest.recv()).await });
            let frame = frame
                .ok()
                .flatten()
                .ok_or("no frame came in 10 s: is the camera streaming?")?;
            let image = preview.linear(&frame).map_err(|e| e.to_string())?;
            let (width, height) = (image.width(), image.height());
            let mut values = Values::from([("image".to_owned(), Value::image(image))]);
            if region {
                let whole = json!({ "x": 0, "y": 0, "width": width, "height": height, "frame_width": width, "frame_height": height });
                values.insert("region".to_owned(), Value::json(whole));
            }
            Ok(values)
        });
        let (folder, saved) = (name.clone(), out.clone());
        let write = Box::new(move |item: usize, run: Run| {
            std::fs::create_dir_all(&saved).map_err(|e| e.to_string())?;
            let mut written = Vec::new();
            for (port, value) in &run.outputs {
                let Some(image) = value.as_image() else {
                    continue;
                };
                let channels = image.channels();
                let byte = |sample: f32| (sample.clamp(0.0, 1.0) * 255.0).round() as u8;
                let rgb: Vec<u8> = image
                    .data()
                    .chunks(channels)
                    .flat_map(|p| [0, 1, 2].map(|c| byte(p[c.min(channels - 1)])))
                    .collect();
                let mut png = Vec::new();
                encode_png(&mut png, &rgb, image.width(), image.height())
                    .map_err(|e| e.to_string())?;
                let file = format!("frame-{}-{port}.png", item + 1);
                std::fs::write(saved.join(&file), png).map_err(|e| e.to_string())?;
                written.push(format!("batch/{folder}/{file}"));
            }
            Ok(Some(written.join(", ")).filter(|w| !w.is_empty()))
        });
        let title = match recipe.name.is_empty() {
            true => format!("a recipe over {frames} frames"),
            false => format!("{} over {frames} frames", recipe.name),
        };
        let batch = Batch {
            count: frames,
            read,
            write,
            tile,
        };
        Ok(engine.batch_job(&title, Arc::new(plan), batch, 0))
    })
}
