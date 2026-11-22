//! The simulator's stained sections (P45): colour deconvolution gives back
//! their stains' amounts pixel by pixel; Macenko's estimate finds H&E's
//! vectors in the H&E section; and the H-DAB section's DAB lies in 40 % of
//! its nuclei, at three levels, as its truth says.

use oscope_core::{Point2, Rect, Um};
use oscope_imaging::stain::{
    DAB, DabThresholds, EOSIN, HEMATOXYLIN, Stains, Vector, dab_class, macenko,
};
use oscope_sim::{HeTissue, Sample, Slide};

/// The light the section passes over `w` × `h` pixels of `pixel` µm from
/// `origin`, of a white of 1: its layers' densities summed, as 10^(−OD).
fn passed(tissue: &HeTissue, origin: Point2, pixel: f64, (w, h): (u32, u32)) -> Vec<f32> {
    let mut od = vec![0.0_f32; (w * h * 3) as usize];
    for layer in 0..tissue.layers().len() {
        let image = tissue.render(layer, origin, Um(pixel), w, h);
        for (o, v) in od.iter_mut().zip(image.data()) {
            *o += v;
        }
    }
    od.iter().map(|v| 10f32.powf(-v)).collect()
}

/// A region about the first island's middle.
fn about_island(tissue: &HeTissue, pixel: f64, (w, h): (u32, u32)) -> Point2 {
    let island = tissue.islands()[0];
    let half = (f64::from(w) * pixel / 2.0, f64::from(h) * pixel / 2.0);
    Point2::new(island.x - half.0, island.y - half.1)
}

#[test]
fn deconvolution_gives_back_each_section_s_stains_pixel_by_pixel() {
    let sections = [
        (HeTissue::new(1, Slide::COVERSLIP), Stains::he()),
        (HeTissue::ihc(1, Slide::COVERSLIP), Stains::hdab()),
    ];
    for (tissue, stains) in sections {
        let (pixel, size) = (0.5, (160, 120));
        let origin = about_island(&tissue, pixel, size);
        let amounts = stains.deconvolve(&passed(&tissue, origin, pixel, size), [1.0; 3]);
        let (mut worst, mut most) = (0.0_f64, [0.0_f64; 2]);
        for j in 0..size.1 {
            for i in 0..size.0 {
                let at = Point2::new(
                    origin.x + (f64::from(i) + 0.5) * pixel,
                    origin.y + (f64::from(j) + 0.5) * pixel,
                );
                let truth = tissue.stain_amounts(at, pixel);
                let k = ((j * size.0 + i) * 3) as usize;
                for s in 0..2 {
                    worst = worst.max((f64::from(amounts[k + s]) - truth[s]).abs());
                    most[s] = most[s].max(truth[s]);
                }
            }
        }
        assert!(worst < 1e-4, "{}: {worst}", tissue.name());
        // There was something of each stain to find.
        assert!(
            most[0] > 0.5 && most[1] > 0.2,
            "{}: {most:?}",
            tissue.name()
        );
    }
}

/// The angle between two vectors, in degrees.
fn degrees(a: Vector, b: Vector) -> f64 {
    let n = |v: Vector| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let cos = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / (n(a) * n(b));
    cos.clamp(-1.0, 1.0).acos().to_degrees()
}

#[test]
fn macenko_finds_the_h_and_e_section_s_vectors() {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let (pixel, size) = (0.5, (400, 300));
    let origin = about_island(&tissue, pixel, size);
    // Its nuclei hold eosin and its stroma hematoxylin, so no pixel is one
    // stain alone, and each vector leans towards the other: 5.7° and 5.3°.
    let [h, e] = macenko(&passed(&tissue, origin, pixel, size), [1.0; 3], 0.15, 1.0).unwrap();
    let off = |v: Vector, w: Vector| degrees(v, w);
    assert!(off(h, HEMATOXYLIN) < 7.0, "{h:?}: {}°", off(h, HEMATOXYLIN));
    assert!(off(e, EOSIN) < 7.0, "{e:?}: {}°", off(e, EOSIN));
    // The H-DAB section's stroma is hematoxylin alone, which is found; its
    // DAB lies only in nuclei that hold hematoxylin too: 16.5° off.
    let tissue = HeTissue::ihc(1, Slide::COVERSLIP);
    let [h, d] = macenko(&passed(&tissue, origin, pixel, size), [1.0; 3], 0.15, 1.0).unwrap();
    assert!(off(h, HEMATOXYLIN) < 1.0, "{h:?}: {}°", off(h, HEMATOXYLIN));
    assert!(off(d, DAB) < 20.0, "{d:?}: {}°", off(d, DAB));
}

#[test]
fn the_h_dab_section_s_dab_lies_in_two_fifths_of_its_nuclei() {
    let (he, ihc) = (
        HeTissue::new(1, Slide::COVERSLIP),
        HeTissue::ihc(1, Slide::COVERSLIP),
    );
    let island = ihc.islands()[0];
    let region = Rect::new(island.x - 600.0, island.y - 600.0, 1_200.0, 1_200.0);
    let nuclei = ihc.nuclei(region);
    assert!(nuclei.len() > 1_000, "{}", nuclei.len());
    // The same nuclei, none with DAB in H&E.
    assert_eq!(he.nuclei(region).len(), nuclei.len());
    assert!(he.nuclei(region).iter().all(|n| n.dab == 0.0));
    let share = |f: &dyn Fn(f64) -> bool| {
        nuclei.iter().filter(|n| f(n.dab)).count() as f64 / nuclei.len() as f64
    };
    let positive = share(&|d| d > 0.0);
    assert!((positive - 0.4).abs() < 0.04, "{positive}");
    for level in [0.5, 1.0, 1.6] {
        let s = share(&|d| d == level);
        assert!((s - 0.4 / 3.0).abs() < 0.03, "{level}: {s}");
    }
    // DAB lies only in the positive nuclei: none in a 2 µm grid's points
    // farther from them than their long axes.
    let positive: Vec<_> = nuclei.iter().filter(|n| n.dab > 0.0).collect();
    let mut stained = 0;
    for j in 0..100 {
        for i in 0..100 {
            let at = Point2::new(island.x + 2.0 * f64::from(i), island.y + 2.0 * f64::from(j));
            if ihc.stain_amounts(at, 0.25)[1] > 1e-6 {
                stained += 1;
                let near = |n: &&oscope_sim::Nucleus| {
                    (at.x - n.centre.x).hypot(at.y - n.centre.y) <= n.semi_major.0 + 0.5
                };
                assert!(positive.iter().any(near), "DAB at {at:?}");
            }
        }
    }
    assert!(stained > 100, "{stained}");
    // At its middle, each level's typical nucleus has the DAB of its bin:
    // weak, moderate, strong.
    let thresholds = DabThresholds::default();
    for (level, class) in [(0.5, 2), (1.0, 3), (1.6, 4)] {
        let mut amounts: Vec<f64> = nuclei
            .iter()
            .filter(|n| n.dab == level)
            .map(|n| ihc.stain_amounts(n.centre, 0.25)[1])
            .collect();
        amounts.sort_by(f64::total_cmp);
        let median = amounts[amounts.len() / 2];
        assert_eq!(
            dab_class(1.0, median as f32, &thresholds),
            class,
            "{level}: {median}"
        );
    }
}
