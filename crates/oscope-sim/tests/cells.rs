//! The fluorescent cell culture: its ground-truth volume against what it
//! renders, its cells' shapes, and its determinism.

use oscope_core::{Point2, Rect, Um};
use oscope_sim::cells::{DENSITY, DYES};
use oscope_sim::{CellCulture, Sample};

fn culture(seed: u64) -> CellCulture {
    CellCulture::new(seed, Rect::new(20_000.0, 5_000.0, 1_000.0, 1_000.0))
}

/// A cell near the culture's middle.
fn middle(culture: &CellCulture) -> oscope_sim::Cell {
    culture.cells(Rect::new(20_400.0, 5_400.0, 200.0, 200.0))[0].clone()
}

#[test]
fn the_truth_holds_what_the_slices_render() {
    // Over a cell, at 0.1 µm pixels: for each dye, the slices' amounts,
    // summed through the depth, against the truth's densities integrated
    // through it in steps of 0.02 µm.
    let culture = culture(3);
    let cell = middle(&culture);
    let (pixel, size) = (Um(0.1), 64);
    let origin = Point2::new(cell.centre.x - 3.2, cell.centre.y - 3.2);
    let slices = culture.layers().len() / DYES.len();
    let mut rendered = [0.0_f64; 3];
    for index in 0..culture.layers().len() {
        let layer = culture.render(index, origin, pixel, size, size);
        rendered[index % DYES.len()] += layer
            .data()
            .iter()
            .step_by(3)
            .map(|v| f64::from(*v))
            .sum::<f64>();
    }
    let dz = 0.02;
    let mut truth = [0.0_f64; 3];
    let planes = (CellCulture::TOP.0 / dz).round() as usize;
    for k in 0..planes {
        let plane = culture.truth(origin, pixel, size, size, Um((k as f64 + 0.5) * dz));
        for (i, v) in plane.data().iter().enumerate() {
            truth[i % 3] += f64::from(*v) * dz;
        }
    }
    assert_eq!(slices, 13);
    for d in 0..3 {
        assert!(truth[d] > 0.0, "{:?}: none in the truth", DYES[d]);
        assert!(
            (rendered[d] / truth[d] - 1.0).abs() < 0.01,
            "{:?}: rendered {} against the truth's {}",
            DYES[d],
            rendered[d],
            truth[d]
        );
    }
}

#[test]
fn a_coarse_slice_holds_as_much_dye_as_a_fine_one() {
    // Over 96 µm of the culture, at 1 µm pixels, the membranes are widened
    // to a pixel and the organelles to a pixel across, with their density
    // lowered: each dye's amount, through every slice, against 0.2 µm
    // pixels, where nothing is widened. The organelles, under a pixel
    // across, fall on pixel centres by chance, so come within 3%.
    let culture = culture(3);
    let origin = Point2::new(20_450.0, 5_450.0);
    for (d, within) in [(0, 0.01), (1, 0.01), (2, 0.03)] {
        let total = |p: f64, n: u32| {
            (0..culture.layers().len() / DYES.len())
                .map(|k| {
                    let layer = culture.render(k * DYES.len() + d, origin, Um(p), n, n);
                    layer
                        .data()
                        .iter()
                        .step_by(3)
                        .map(|v| f64::from(*v))
                        .sum::<f64>()
                        * p
                        * p
                })
                .sum::<f64>()
        };
        let (fine, coarse) = (total(0.2, 480), total(1.0, 96));
        assert!(
            (coarse / fine - 1.0).abs() < within,
            "{:?}: {coarse} at 1 µm against {fine} at 0.2 µm",
            DYES[d]
        );
    }
}

#[test]
fn cells_hold_their_nuclei_and_organelles_and_keep_apart() {
    let culture = culture(3);
    let cells = culture.cells(Rect::new(20_000.0, 5_000.0, 1_000.0, 1_000.0));
    // Seven grid squares in ten, of 31 × 31 inside the culture's edge.
    assert!((550..800).contains(&cells.len()), "{} cells", cells.len());
    let inside = |c: &oscope_sim::Cell, x: f64, y: f64, z: f64| {
        let (dx, dy) = (x - c.centre.x, y - c.centre.y);
        let (sin, cos) = c.angle.sin_cos();
        let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        let rho = (u / c.radii[0]).powi(2) + (v / c.radii[1]).powi(2);
        rho < 1.0 && z >= 0.0 && z <= c.radii[2] * (1.0 - rho).sqrt()
    };
    for cell in &cells {
        let n = cell.nucleus;
        // The nucleus's top, and its ends along the cell's long axis.
        let (sin, cos) = cell.angle.sin_cos();
        let [x, y, z] = n.centre;
        assert!(inside(cell, x, y, z + n.radii[2]), "{cell:?}");
        for s in [-1.0, 1.0] {
            assert!(
                inside(cell, x + s * n.radii[0] * cos, y + s * n.radii[0] * sin, z),
                "{cell:?}"
            );
        }
        assert!(
            cell.organelles.len() >= 8,
            "{} organelles",
            cell.organelles.len()
        );
        for o in &cell.organelles {
            let [x, y, z] = o.centre;
            assert!(inside(cell, x, y, z));
            let (dx, dy) = (x - n.centre[0], y - n.centre[1]);
            let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
            let in_nucleus = (u / n.radii[0]).powi(2)
                + (v / n.radii[1]).powi(2)
                + ((z - n.centre[2]) / n.radii[2]).powi(2);
            assert!(in_nucleus > 1.0, "an organelle in the nucleus");
        }
    }
    let organelles = cells.iter().map(|c| c.organelles.len()).sum::<usize>() as f64;
    let each = organelles / cells.len() as f64;
    assert!((20.0..35.0).contains(&each), "{each} organelles a cell");
    // No two cells overlap: their centres are further apart than their long
    // half-lengths together.
    for (i, a) in cells.iter().enumerate() {
        for b in &cells[i + 1..] {
            assert!(a.centre.distance(b.centre) > a.radii[0] + b.radii[0]);
        }
    }
    // The truth's densities are the dyes' own, or none.
    let plane = culture.truth(cells[0].centre, Um(0.2), 32, 32, Um(2.0));
    let dyes_own = |v: &[f32]| (0..3).all(|d| v[d] == 0.0 || v[d] == DENSITY[d] as f32);
    assert!(plane.data().chunks(3).all(dyes_own));
}

#[test]
fn the_same_seed_gives_the_same_culture() {
    // The membranes' fifth slice, from 2 to 2.5 µm, across a cell, from
    // seeds 3, 3 and 4; and its right half, rendered alone.
    let cell = middle(&culture(3));
    let origin = Point2::new(cell.centre.x - 9.6, cell.centre.y - 4.8);
    let layer = |seed: u64, origin: Point2, width: u32| {
        culture(seed)
            .render(4 * DYES.len() + 1, origin, Um(0.1), width, 96)
            .data()
            .to_vec()
    };
    let whole = layer(3, origin, 192);
    assert!(whole.iter().any(|v| *v > 0.0));
    assert_eq!(whole, layer(3, origin, 192));
    assert_ne!(whole, layer(4, origin, 192));
    let half = layer(3, Point2::new(origin.x + 9.6, origin.y), 96);
    let right: Vec<f32> = whole
        .chunks(192 * 3)
        .flat_map(|row| row[96 * 3..].to_vec())
        .collect();
    assert_eq!(half, right);
}
