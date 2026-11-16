//! Charts drawn as SVG: bars or a line, with their axes, ticks and labels.
//! Their numbers are written to two decimals, so a chart draws the same
//! each time.

use std::fmt::Write as _;

use crate::{Chart, ChartKind};

const WIDTH: f64 = 480.0;
const HEIGHT: f64 = 260.0;
const LEFT: f64 = 56.0;
const BOTTOM: f64 = 44.0;
const TOP: f64 = 12.0;
const RIGHT: f64 = 12.0;

/// Escapes `text` for SVG.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A round number for the axis's top: 1, 2 or 5 times a power of ten, at
/// least `most`.
fn nice(most: f64) -> f64 {
    if most <= 0.0 || !most.is_finite() {
        return 1.0;
    }
    let power = 10f64.powf(most.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * power)
        .find(|&v| v >= most)
        .unwrap_or(10.0 * power)
}

/// `chart` as SVG.
pub fn svg(chart: &Chart) -> String {
    let (w, h) = (WIDTH - LEFT - RIGHT, HEIGHT - TOP - BOTTOM);
    let top = nice(chart.values.iter().copied().fold(0.0, f64::max));
    let n = chart.values.len().max(1) as f64;
    let y = |v: f64| TOP + h * (1.0 - v.max(0.0) / top);
    let x = |i: usize| LEFT + w * (i as f64 + 0.5) / n;
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}" width="{WIDTH}" height="{HEIGHT}" font-family="Roboto, sans-serif" font-size="11">"#
    );
    // The axes, and ticks at 0, half and the top.
    let _ = write!(
        s,
        r##"<path d="M{LEFT:.2} {TOP:.2}V{:.2}H{:.2}" fill="none" stroke="#555"/>"##,
        TOP + h,
        LEFT + w
    );
    for v in [0.0, top / 2.0, top] {
        let _ = write!(
            s,
            r#"<text x="{:.2}" y="{:.2}" text-anchor="end">{}</text>"#,
            LEFT - 6.0,
            y(v) + 4.0,
            v
        );
    }
    let step = (chart.labels.len() / 16).max(1);
    for (i, label) in chart.labels.iter().enumerate().step_by(step) {
        let _ = write!(
            s,
            r#"<text x="{:.2}" y="{:.2}" text-anchor="middle">{}</text>"#,
            x(i),
            TOP + h + 16.0,
            escaped(label)
        );
    }
    match chart.kind {
        ChartKind::Bars => {
            let bar = 0.8 * w / n;
            for (i, &v) in chart.values.iter().enumerate() {
                let _ = write!(
                    s,
                    r##"<rect x="{:.2}" y="{:.2}" width="{bar:.2}" height="{:.2}" fill="#4c78a8"/>"##,
                    x(i) - bar / 2.0,
                    y(v),
                    TOP + h - y(v)
                );
            }
        }
        ChartKind::Line => {
            let points: Vec<String> = chart
                .values
                .iter()
                .enumerate()
                .map(|(i, &v)| format!("{:.2},{:.2}", x(i), y(v)))
                .collect();
            let _ = write!(
                s,
                r##"<polyline points="{}" fill="none" stroke="#4c78a8" stroke-width="2"/>"##,
                points.join(" ")
            );
        }
    }
    let _ = write!(
        s,
        r#"<text x="{:.2}" y="{:.2}" text-anchor="middle">{}</text>"#,
        LEFT + w / 2.0,
        HEIGHT - 6.0,
        escaped(&chart.x_label)
    );
    let _ = write!(
        s,
        r#"<text transform="translate(14 {:.2}) rotate(-90)" text-anchor="middle">{}</text>"#,
        TOP + h / 2.0,
        escaped(&chart.y_label)
    );
    s + "</svg>"
}
