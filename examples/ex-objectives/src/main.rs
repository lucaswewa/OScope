//! `ex-objectives`: the objective catalogue, with each objective's pixel
//! size, field of view and resolution on the MIChrome 20; then the manual
//! objective mount and lights, with their capability reports. With
//! `--smoke`, it also checks what it prints. See the README.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use oscope_core::{LightState, Um};
use oscope_hal::{
    Illumination, ManualIllumination, ManualObjectiveMount, ObjectiveCatalogue, ObjectiveMount,
};

/// The MIChrome 20's sensor, in pixels, and its pixel.
const SENSOR: (u32, u32) = (5472, 3648);
const PIXEL: Um = Um(2.4);
/// Green light, for the resolution.
const WAVELENGTH: Um = Um(0.55);

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("ex-objectives: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the example; `false` if a `--smoke` check failed.
async fn run() -> anyhow::Result<bool> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smoke = args.iter().any(|a| a == "--smoke");
    let file = match args.iter().position(|a| a == "--catalogue") {
        Some(i) => Some(args.get(i + 1).context("--catalogue needs a file")?),
        None => None,
    };
    let mut failures = Vec::new();
    let mut check = |ok: bool, what: &str| {
        if !ok {
            failures.push(what.to_owned());
        }
    };

    let (catalogue, source) = match file {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
            (ObjectiveCatalogue::from_toml(&text)?, path.clone())
        }
        None => (ObjectiveCatalogue::mitutoyo(), "Mitutoyo's defaults".into()),
    };
    println!(
        "The objective catalogue: {source}, behind a {} mm tube lens",
        catalogue.tube_lens.mm()
    );
    println!(
        "On the MIChrome 20, {} × {} pixels of {PIXEL}, in light of {WAVELENGTH}:\n",
        SENSOR.0, SENSOR.1
    );
    println!(
        "  {:<6} {:>5} {:>9}  {:>10}  {:>15}  {:>10}  model",
        "name", "NA", "working", "pixel", "field of view", "resolution"
    );
    let mut rows = Vec::new();
    for o in &catalogue.objectives {
        let pixel = catalogue.pixel_size(o, PIXEL);
        let field = (pixel * f64::from(SENSOR.0), pixel * f64::from(SENSOR.1));
        // Rayleigh's criterion: two points 0.61 λ / NA apart are just resolved.
        let resolution = Um(0.61 * WAVELENGTH.0 / o.na);
        println!(
            "  {:<6} {:>5.2} {:>6.1} mm  {:>7.3} µm  {:>5.2} × {:>4.2} mm  {:>7.2} µm  {}",
            o.name,
            o.na,
            o.working_distance.mm(),
            pixel.0,
            field.0.mm(),
            field.1.mm(),
            resolution.0,
            o.model
        );
        rows.push((o.name.clone(), pixel.0, field.0.mm(), resolution.0));
    }

    println!("\nThe manual objective mount: the user swaps objectives by hand, and says which");
    let mount: Arc<dyn ObjectiveMount> = Arc::new(ManualObjectiveMount::new(catalogue.clone()));
    println!(
        "  capabilities: {}",
        serde_json::to_string(mount.capabilities())?
    );
    let name = catalogue.objectives[catalogue.objectives.len() / 2]
        .name
        .clone();
    let fitted = mount.fit(name.clone()).await?;
    println!("  fitted {:?}: {}", fitted.name, fitted.model);
    let refused = mount.fit("7x".into()).await;
    if let Err(error) = &refused {
        println!("  fitting \"7x\": {error}");
    }
    let still = mount.fitted().await?.map(|o| o.name);
    println!("  fitted now: {still:?}");

    println!("\nThe manual lights: the user switches them, and OScope records what they say");
    let lights: Arc<dyn Illumination> = Arc::new(ManualIllumination::brightfield());
    println!(
        "  capabilities: {}",
        serde_json::to_string(lights.capabilities())?
    );
    let on = LightState {
        channel: "transmitted".into(),
        on: true,
        intensity: 0.6,
    };
    lights.set(on.clone()).await?;
    let too_bright = lights
        .set(LightState {
            intensity: 1.5,
            ..on.clone()
        })
        .await;
    if let Err(error) = &too_bright {
        println!("  setting an intensity of 1.5: {error}");
    }
    let state = lights.state().await?;
    println!("  state: {}", serde_json::to_string(&state)?);

    if !smoke {
        return Ok(true);
    }
    // The defaults, and the shipped file, match plan §5.9.
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/objectives.toml");
    let shipped = ObjectiveCatalogue::from_toml(&std::fs::read_to_string(shipped)?)?;
    check(
        shipped == ObjectiveCatalogue::mitutoyo(),
        "the shipped catalogue",
    );
    if file.is_none() {
        let expected = [
            ("5x", 0.48, 2.62656, 2.396),
            ("10x", 0.24, 1.31328, 1.198),
            ("20x", 0.12, 0.65664, 0.799),
            ("50x", 0.048, 0.262656, 0.610),
        ];
        let matches = rows.len() == expected.len()
            && rows.iter().zip(expected).all(|(row, want)| {
                row.0 == want.0
                    && (row.1 - want.1).abs() < 1e-9
                    && (row.2 - want.2).abs() < 1e-9
                    && (row.3 - want.3).abs() < 1e-3
            });
        check(matches, "the pixel sizes, fields of view and resolutions");
    }
    check(
        fitted.name == name && refused.is_err(),
        "fitting objectives",
    );
    check(still == Some(name), "the fitted objective after a refusal");
    check(too_bright.is_err() && state == [on], "setting the light");
    if !failures.is_empty() {
        eprintln!("\nsmoke: failed: {}", failures.join(", "));
        return Ok(false);
    }
    println!("\nsmoke: ok");
    Ok(true)
}
