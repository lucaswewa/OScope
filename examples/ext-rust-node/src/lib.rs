//! A sample Rust extension for OScope (P25, ADR-0019): one node,
//! `example.emboss`, which embosses an image as if lit from its top left.
//!
//! It is built as a `cdylib`, packed with `manifest.toml` into an `.omext`,
//! and installed. OScope loads it at its next start. `ex-rust-ext` does all
//! of that.

pub mod pgm;

use oscope_ext_sdk::{Descriptor, Image, Outputs, ProcessingNode, serde_json};

/// The weights, row by row from the top: a difference across the diagonal
/// from the top left to the bottom right. They sum to 0, so a flat image
/// gives a flat grey.
const WEIGHTS: [[f32; 3]; 3] = [[-2.0, -1.0, 0.0], [-1.0, 0.0, 1.0], [0.0, 1.0, 2.0]];

/// Embosses `image` into `out`, which has its shape. Each sample becomes
/// 0.5 plus `strength` times the weighted sum of its 3 × 3 neighbourhood
/// over 4, the edges repeated: beside a step of 1 across the diagonal,
/// 0.5 + 0.75 × `strength`.
pub fn emboss(image: &Image<'_>, strength: f32, out: &mut [f32]) {
    let (width, channels) = (image.width() as usize, image.channels());
    for y in 0..i64::from(image.height()) {
        for x in 0..width as i64 {
            for c in 0..channels {
                let mut sum = 0.0;
                for (dy, row) in (-1..=1).zip(WEIGHTS) {
                    for (dx, weight) in (-1..=1).zip(row) {
                        sum += weight * image.clamped(x + dx, y + dy, c);
                    }
                }
                out[(y as usize * width + x as usize) * channels + c] = 0.5 + strength * sum / 4.0;
            }
        }
    }
}

/// The node: `example.emboss`.
pub struct Emboss;

impl ProcessingNode for Emboss {
    fn descriptor(&self) -> Descriptor {
        let params = serde_json::json!({
            "type": "object",
            "properties": {
                "strength": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 10,
                    "default": 1,
                    "description": "How deep the relief is."
                }
            },
            "additionalProperties": false
        });
        let what = "Embosses an image, as if lit from its top left.";
        Descriptor::new("example.emboss", 1, "example", what)
            .input("image")
            .output("image")
            .params(params)
            .live()
            .tileable(1)
    }

    fn run(
        &self,
        params: &serde_json::Value,
        inputs: &[Image<'_>],
        outputs: &mut Outputs<'_>,
    ) -> Result<(), String> {
        let strength = params["strength"].as_f64().unwrap_or(1.0) as f32;
        let image = inputs.first().ok_or("no image")?;
        let out = outputs.image(0, image.width(), image.height(), image.channels())?;
        emboss(image, strength, out);
        Ok(())
    }
}

oscope_ext_sdk::export!(
    nodes: [Emboss],
    drivers: [
        oscope_ext_sdk::importer("example.pgm.read", pgm::PgmReader),
        oscope_ext_sdk::exporter("example.pgm.write", pgm::PgmWriter::default()),
    ],
);

#[cfg(test)]
mod tests {
    use oscope_ext_sdk::run_in_memory;
    use oscope_ext_sdk::serde_json::json;

    use super::*;

    #[test]
    fn a_flat_image_is_grey_and_a_step_stands_out() {
        let flat = [0.3; 12];
        let image = Image::new(4, 3, 1, &flat).unwrap();
        let made = run_in_memory(&Emboss, &json!({}), &[image]).unwrap();
        assert!(
            made[0].data.iter().all(|s| (s - 0.5).abs() < 1e-6),
            "{:?}",
            made[0].data
        );

        // Dark in the left two columns, light in the right two: the columns
        // beside the step stand out by 0.75 × strength.
        let step: Vec<f32> = (0..12)
            .map(|i| if i % 4 >= 2 { 1.0 } else { 0.0 })
            .collect();
        let image = Image::new(4, 3, 1, &step).unwrap();
        let made = run_in_memory(&Emboss, &json!({ "strength": 2.0 }), &[image]).unwrap();
        assert_eq!(made[0].data, [0.5, 2.0, 2.0, 0.5].repeat(3));
    }
}
