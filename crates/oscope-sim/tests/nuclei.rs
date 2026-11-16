//! Cells against the simulator's truth (P46): the H&E section rendered at
//! the size of 20×'s cells, segmented on its hematoxylin, and its objects
//! matched to the nuclei the simulator put there.

use oscope_core::{Point2, Rect, Um};
use oscope_imaging::cells::{Filters, Threshold, measure, segment};
use oscope_imaging::stain::Stains;
use oscope_sim::{HeTissue, Sample, Slide};

/// The light the section passes over `w` × `h` pixels of `pixel` µm from
/// `origin`, of a white of 1.
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

/// Pairs of `found` and `truth`, nearest first, each used once, within
/// `within` µm: their indices and distance.
fn matched(found: &[[f64; 2]], truth: &[[f64; 2]], within: f64) -> Vec<(usize, usize, f64)> {
    let mut pairs: Vec<(usize, usize, f64)> = found
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            truth.iter().enumerate().filter_map(move |(j, t)| {
                let d = (f[0] - t[0]).hypot(f[1] - t[1]);
                (d <= within).then_some((i, j, d))
            })
        })
        .collect();
    pairs.sort_by(|a, b| a.2.total_cmp(&b.2));
    let (mut used_f, mut used_t) = (vec![false; found.len()], vec![false; truth.len()]);
    pairs
        .into_iter()
        .filter(|&(i, j, _)| {
            let free = !used_f[i] && !used_t[j];
            if free {
                (used_f[i], used_t[j]) = (true, true);
            }
            free
        })
        .collect()
}

#[test]
fn nuclei_are_found_where_the_simulator_put_them() {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let (pixel, (w, h)) = (0.96, (480_u32, 360_u32));
    let island = tissue.islands()[0];
    let size = (f64::from(w) * pixel, f64::from(h) * pixel);
    let origin = Point2::new(island.x - size.0 / 2.0, island.y - size.1 / 2.0);
    let amounts = Stains::he().deconvolve(&passed(&tissue, origin, pixel, (w, h)), [1.0; 3]);
    let hematoxylin: Vec<f32> = amounts.iter().step_by(3).copied().collect();
    let (labels, count) = segment(&hematoxylin, w as usize, Threshold::Otsu, 2, 1.5);
    let objects = measure(&labels, w as usize, count, [pixel, pixel], &[]);
    let filters = Filters::default();
    let found: Vec<[f64; 2]> = objects
        .iter()
        .filter(|o| filters.keep(o))
        .map(|o| [origin.x + o.centroid[0], origin.y + o.centroid[1]])
        .collect();
    // Away from the region's edges, where nuclei are cut.
    let margin = 6.0;
    let inner = |p: &[f64; 2]| {
        (origin.x + margin..origin.x + size.0 - margin).contains(&p[0])
            && (origin.y + margin..origin.y + size.1 - margin).contains(&p[1])
    };
    let nuclei = tissue.nuclei(Rect::new(origin.x, origin.y, size.0, size.1));
    let truth: Vec<[f64; 2]> = nuclei.iter().map(|n| [n.centre.x, n.centre.y]).collect();
    let pairs = matched(&found, &truth, 3.0);
    let found_in = found.iter().filter(|p| inner(p)).count() as f64;
    let truth_in = truth.iter().filter(|p| inner(p)).count() as f64;
    let paired_in = pairs.iter().filter(|&&(_, j, _)| inner(&truth[j])).count() as f64;
    let found_paired_in = pairs.iter().filter(|&&(i, _, _)| inner(&found[i])).count() as f64;
    let (recall, precision) = (paired_in / truth_in, found_paired_in / found_in);
    assert!(truth_in > 300.0, "{truth_in} nuclei");
    assert!(
        recall >= 0.9 && precision >= 0.9,
        "recall {recall:.3}, precision {precision:.3}"
    );
    let mut distances: Vec<f64> = pairs.iter().map(|p| p.2).collect();
    distances.sort_by(f64::total_cmp);
    let median = distances[distances.len() / 2];
    assert!(median < 1.0, "the median miss {median:.2} µm");
    // Touching pairs, centres closer than their long axes: split as often as
    // not, each found apart.
    let mut touching = 0;
    let mut split = 0;
    for (a, na) in nuclei.iter().enumerate() {
        for (b, nb) in nuclei.iter().enumerate().skip(a + 1) {
            let apart = (na.centre.x - nb.centre.x).hypot(na.centre.y - nb.centre.y);
            if apart < na.semi_major.0 + nb.semi_major.0 && inner(&truth[a]) && inner(&truth[b]) {
                touching += 1;
                let both = [a, b].iter().all(|j| pairs.iter().any(|p| p.1 == *j));
                split += usize::from(both);
            }
        }
    }
    assert!(
        touching > 10 && 2 * split >= touching,
        "{split} of {touching} touching pairs split"
    );
}
