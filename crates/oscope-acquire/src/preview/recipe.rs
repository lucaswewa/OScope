//! Live recipes (P22, ADR-0017): a recipe the preview runs on each image,
//! after demosaicing, binning and the black level, in place of the look's
//! colour and display steps.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_compute::{Image, Lane, Shape};
use oscope_pipeline::stock::Region;
use oscope_pipeline::{Engine, Plan, PortType, RunOptions, Value, Values};

use super::PreviewError;

/// A recipe for the preview, checked:
///
/// - every step can run live;
/// - it takes the image as `image`, in linear light, and, if it asks, the
///   part of the frame the image shows as `region`
///   ([`oscope_pipeline::stock::Region`]);
/// - it gives `image`, the same size, in display values from 0 to 1.
#[derive(Clone)]
pub struct LiveRecipe {
    engine: Arc<Engine>,
    plan: Arc<Plan>,
    region: bool,
}

impl fmt::Debug for LiveRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LiveRecipe").field(&self.plan).finish()
    }
}

impl LiveRecipe {
    /// `plan`, for `engine` to run, if it can be a live recipe; why not
    /// otherwise.
    pub fn new(engine: Arc<Engine>, plan: Arc<Plan>) -> Result<LiveRecipe, String> {
        let slow = plan.not_live();
        if !slow.is_empty() {
            return Err(format!("{} can't run live", slow.join(", ")));
        }
        let mut region = false;
        for port in plan.inputs() {
            match (port.name.as_str(), port.kind) {
                ("image", PortType::Image) => {}
                ("region", PortType::Json) => region = true,
                (name, kind) => {
                    let message = format!(
                        "a live recipe takes `image`, and perhaps `region`, not {kind} `{name}`"
                    );
                    return Err(message);
                }
            }
        }
        if !plan.inputs().iter().any(|port| port.name == "image") {
            return Err("a live recipe takes an image as `image`".into());
        }
        if !plan.outputs().contains(&("image", PortType::Image)) {
            return Err("a live recipe gives an image as `image`".into());
        }
        Ok(LiveRecipe {
            engine,
            plan,
            region,
        })
    }

    /// Runs it on `image`, which shows `region` of its frame: the display
    /// values, the same size, and the time it took.
    pub(crate) fn run(
        &self,
        image: Value,
        region: Region,
    ) -> Result<(Image, Duration), PreviewError> {
        let started = Instant::now();
        let failed = |message: String| PreviewError::Recipe(message);
        let size = match &image {
            Value::Image(image) => (image.width(), image.height()),
            #[cfg(feature = "cuda")]
            Value::DeviceImage(image) => (image.shape().width, image.shape().height),
            _ => return Err(failed("the preview's image isn't one".into())),
        };
        let mut inputs = Values::from([("image".to_owned(), image)]);
        if self.region {
            let region = serde_json::to_value(region).unwrap_or_default();
            inputs.insert("region".into(), Value::json(region));
        }
        let options = RunOptions {
            lane: Lane::Live,
            memoise: false,
            ..RunOptions::default()
        };
        let mut run = self
            .engine
            .run(&self.plan, &inputs, &options)
            .map_err(|e| failed(e.to_string()))?;
        let shown = match run.outputs.remove("image") {
            Some(Value::Image(shown)) => Arc::unwrap_or_clone(shown),
            _ => return Err(failed("it gave no image".into())),
        };
        if (shown.width(), shown.height()) != size {
            let (w, h) = (shown.width(), shown.height());
            let message = format!(
                "it gave an image of {w} × {h} for one of {} × {}: a live recipe keeps the size",
                size.0, size.1
            );
            return Err(failed(message));
        }
        Ok((shown, started.elapsed()))
    }

    /// How long a run takes on an RGB image of `width` × `height`: the
    /// middle of three runs, after one to warm up.
    pub fn time(&self, width: u32, height: u32) -> Result<Duration, PreviewError> {
        let shape = Shape::new(width.max(1), height.max(1), 3);
        let data = (0..shape.len()).map(|i| (i % 251) as f32 / 250.0).collect();
        let image = Image::from_vec(shape, data)?;
        let (w, h) = (f64::from(shape.width), f64::from(shape.height));
        let region = Region {
            x: 0.0,
            y: 0.0,
            width: w,
            height: h,
            frame_width: w,
            frame_height: h,
        };
        self.run(Value::image(image.clone()), region)?;
        let mut times = Vec::with_capacity(3);
        for _ in 0..3 {
            times.push(self.run(Value::image(image.clone()), region)?.1);
        }
        times.sort();
        Ok(times[1])
    }
}
