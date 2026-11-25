//! `cargo xtask perf` (P53): plan §5.20's budgets, measured by the
//! benchmarks the phases that set them left, and reported against them.
//! Each benchmark prints what it measured as `perf: {key} {value}` lines.

use serde_json::{Value, json};

/// Which side of its budget a measure must be on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bound {
    /// At most this.
    AtMost(f64),
    /// At least this.
    AtLeast(f64),
}

/// One of §5.20's budgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// The `perf:` key that measures it.
    pub key: &'static str,
    /// What is measured.
    pub what: &'static str,
    /// Its bound.
    pub bound: Bound,
    /// The measure's unit.
    pub unit: &'static str,
    /// The phase that set it.
    pub phase: &'static str,
}

const fn budget(
    key: &'static str,
    what: &'static str,
    bound: Bound,
    unit: &'static str,
    phase: &'static str,
) -> Budget {
    Budget {
        key,
        what,
        bound,
        unit,
        phase,
    }
}

/// §5.20's budgets, in its order.
pub const BUDGETS: &[Budget] = &[
    budget(
        "preview.cpu_ms",
        "Preview: a 20 MP frame to a JPEG at 1920 px, CPU",
        Bound::AtMost(60.0),
        "ms",
        "P17",
    ),
    budget(
        "preview.gpu_ms",
        "Preview: a 20 MP frame to a JPEG at 1920 px, CUDA",
        Bound::AtMost(30.0),
        "ms",
        "P17",
    ),
    budget(
        "demosaic.superpixel.cpu_ms",
        "Demosaic, 20 MP, superpixel, 8 CPU threads",
        Bound::AtMost(80.0),
        "ms",
        "P08",
    ),
    budget(
        "demosaic.bilinear.cpu_ms",
        "Demosaic, 20 MP, bilinear, 8 CPU threads",
        Bound::AtMost(80.0),
        "ms",
        "P08",
    ),
    budget(
        "demosaic.malvar-he-cutler.cpu_ms",
        "Demosaic, 20 MP, Malvar–He–Cutler, 8 CPU threads",
        Bound::AtMost(80.0),
        "ms",
        "P08",
    ),
    budget(
        "demosaic.superpixel.gpu_ms",
        "Demosaic, 20 MP, superpixel, CUDA",
        Bound::AtMost(10.0),
        "ms",
        "P08c",
    ),
    budget(
        "demosaic.bilinear.gpu_ms",
        "Demosaic, 20 MP, bilinear, CUDA",
        Bound::AtMost(10.0),
        "ms",
        "P08c",
    ),
    budget(
        "demosaic.malvar-he-cutler.gpu_ms",
        "Demosaic, 20 MP, Malvar–He–Cutler, CUDA",
        Bound::AtMost(10.0),
        "ms",
        "P08c",
    ),
    // The recorder can't outrun its source's 580 MB/s, and its first
    // frame's wait counts against it: within 1 %, it keeps up.
    budget(
        "record.mb_per_s",
        "Scratch recording, keeping up with the design rate, 580 MB/s",
        Bound::AtLeast(574.2),
        "MB/s",
        "P29",
    ),
    budget(
        "record.dropped",
        "Scratch recording: frames dropped",
        Bound::AtMost(0.0),
        "frames",
        "P29",
    ),
    budget(
        "record.seconds",
        "Scratch recording: for 10 minutes",
        Bound::AtLeast(600.0),
        "s",
        "P29",
    ),
    budget(
        "sim.cpu_fps",
        "Simulated camera at full resolution, CPU",
        Bound::AtLeast(5.0),
        "fps",
        "P15",
    ),
    budget(
        "sim.gpu_fps",
        "Simulated camera at full resolution, CUDA",
        Bound::AtLeast(14.5),
        "fps",
        "P15",
    ),
    budget(
        "register.gpu_ms",
        "Phase correlation of a 2048² pair, CUDA",
        Bound::AtMost(15.0),
        "ms",
        "P31",
    ),
    budget(
        "stitch.tiles_per_s",
        "Post-stitching of 20 MP tiles",
        Bound::AtLeast(10.0),
        "tiles/s",
        "P39",
    ),
    budget(
        "stitch.peak_mb",
        "Post-stitching: its parts at once",
        Bound::AtMost(256.0),
        "MB",
        "P39",
    ),
    budget(
        "zoom.cold_ms",
        "Deep zoom: a cold tile, server time",
        Bound::AtMost(5.0),
        "ms",
        "P40",
    ),
    budget(
        "zoom.kept_ms",
        "Deep zoom: a cached tile, server time",
        Bound::AtMost(1.0),
        "ms",
        "P40",
    ),
    budget(
        "zoom.view_ms",
        "Deep zoom: a full-screen view never seen",
        Bound::AtMost(150.0),
        "ms",
        "P40",
    ),
    budget(
        "zoom.view_held_ms",
        "Deep zoom: the same, a hardware job running",
        Bound::AtMost(150.0),
        "ms",
        "P40",
    ),
    // The app's (W10.3, ADR-0059), measured in Chromium by `--web`.
    budget(
        "web.live.fps",
        "The app: the live view's rate, 1920 × 1272 in mode design",
        Bound::AtLeast(12.0),
        "fps",
        "W1.5",
    ),
    budget(
        "web.live.latency_ms",
        "The app: a frame on the page after its exposure",
        Bound::AtMost(100.0),
        "ms",
        "W1.5",
    ),
    budget(
        "web.first_frame_ms",
        "The app: the first frame after opening it",
        Bound::AtMost(2000.0),
        "ms",
        "W10.3",
    ),
    budget(
        "web.deep_zoom.median_ms",
        "The app: a full-screen view never seen, deep-zoomed, median",
        Bound::AtMost(150.0),
        "ms",
        "W6.4",
    ),
    budget(
        "web.bundle.entry_kb",
        "The app's bundle: its entry, gzipped",
        Bound::AtMost(100.0),
        "kB",
        "W10.3",
    ),
    budget(
        "web.bundle.largest_kb",
        "The app's bundle: its largest chunk, gzipped",
        Bound::AtMost(300.0),
        "kB",
        "W10.3",
    ),
    budget(
        "web.bundle.total_kb",
        "The app's bundle: all of it, gzipped",
        Bound::AtMost(1200.0),
        "kB",
        "W10.3",
    ),
    budget(
        "web.memory.mb_per_hour",
        "The app: its memory's growth over a long live view",
        Bound::AtMost(10.0),
        "MB/h",
        "W10.3",
    ),
];

/// A benchmark: its name, and the `cargo` arguments that run it, quick or
/// whole, with CUDA or not, its scratch in `scratch`.
pub struct Bench {
    /// Its name.
    pub name: &'static str,
    /// Whether it needs CUDA for its GPU measures.
    pub cuda: bool,
    /// Its `cargo` arguments, for a quick run or not, and the features.
    pub args: fn(quick: bool, scratch: &str) -> Vec<String>,
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

/// The benchmarks, each of the phase that set its budgets.
pub const BENCHES: &[Bench] = &[
    Bench {
        name: "preview (P17)",
        cuda: true,
        args: |_, _| {
            words(
                "test --release -p oscope-acquire --test preview -- --ignored --nocapture the_preview_budget",
            )
        },
    },
    Bench {
        name: "demosaic (P08, P08c)",
        cuda: true,
        args: |quick, _| {
            let runs = if quick { 3 } else { 10 };
            words(&format!(
                "run --release -p ex-demosaic -- --smoke --threads 8 --runs {runs}"
            ))
        },
    },
    Bench {
        name: "scratch recording (P29)",
        cuda: false,
        args: |quick, scratch| {
            let seconds = if quick { 30 } else { 600 };
            let mut args = words(&format!(
                "run --release -p ex-record -- --test-sensor --seconds {seconds} --keep-gb 20 --scratch"
            ));
            args.push(scratch.to_owned());
            args
        },
    },
    Bench {
        name: "simulated camera (P15)",
        cuda: true,
        args: |_, _| words("run --release -p ex-sim-bench"),
    },
    Bench {
        name: "registration (P31)",
        cuda: true,
        args: |_, _| words("run --release -p ex-register -- --bench"),
    },
    Bench {
        name: "post-stitching (P39)",
        cuda: false,
        args: |quick, _| {
            let tiles = if quick { 40 } else { 60 };
            words(&format!("run --release -p ex-stitch -- --bench {tiles}"))
        },
    },
    Bench {
        name: "deep zoom (P40)",
        cuda: false,
        args: |quick, _| {
            let megapixels = if quick { 50 } else { 200 };
            words(&format!(
                "run --release -p ex-deep-zoom -- --bench {megapixels}"
            ))
        },
    },
];

/// `args`, with CUDA's feature when asked for and the benchmark has GPU
/// measures: after the package, before `--`.
pub fn with_cuda(mut args: Vec<String>, cuda: bool) -> Vec<String> {
    if cuda {
        let at = args.iter().position(|a| a == "--").unwrap_or(args.len());
        args.splice(at..at, ["--features".to_owned(), "cuda".to_owned()]);
    }
    args
}

/// The `perf:` lines in a benchmark's output: each key and its value.
pub fn measures(output: &str) -> Vec<(String, f64)> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("perf: ")?;
            let (key, value) = rest.split_once(' ')?;
            Some((key.to_owned(), value.trim().parse().ok()?))
        })
        .collect()
}

/// How a measure stands against its budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Within it.
    Met,
    /// Outside it.
    Missed,
    /// Not measured: a GPU's measure without CUDA, or a benchmark that
    /// failed.
    Unmeasured,
}

/// How `measured` stands against `budget`.
pub fn verdict(budget: &Budget, measured: Option<f64>) -> Verdict {
    match (budget.bound, measured) {
        (_, None) => Verdict::Unmeasured,
        (Bound::AtMost(limit), Some(v)) if v <= limit => Verdict::Met,
        (Bound::AtLeast(limit), Some(v)) if v >= limit => Verdict::Met,
        _ => Verdict::Missed,
    }
}

/// A benchmark as it ran.
#[derive(Clone, Debug, PartialEq)]
pub struct Ran {
    /// Its name.
    pub name: String,
    /// Whether it exited well.
    pub ok: bool,
    /// How long it took, in seconds, its build included.
    pub seconds: f64,
}

/// Where and how the benchmarks ran: what the report says of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {
    /// When, as the system says it.
    pub when: String,
    /// The commit.
    pub commit: String,
    /// The computer: its processor, threads, GPU and system.
    pub machine: String,
    /// Whether the runs were the quick ones, shorter than the budgets ask.
    pub quick: bool,
    /// Whether CUDA was built in.
    pub cuda: bool,
}

/// The budgets a quick run doesn't measure: its lengths aren't theirs.
const WHOLE: &[&str] = &["record.seconds", "web.memory.mb_per_hour"];

/// Each budget, what measured it, and its verdict, after a `quick` run or
/// not.
pub fn rows(measured: &[(String, f64)], quick: bool) -> Vec<(Budget, Option<f64>, Verdict)> {
    BUDGETS
        .iter()
        .map(|budget| {
            let value = measured.iter().rev().find(|(k, _)| k == budget.key);
            let whole = !(quick && WHOLE.contains(&budget.key));
            let value = value.map(|(_, v)| *v).filter(|_| whole);
            (*budget, value, verdict(budget, value))
        })
        .collect()
}

/// Whether `rows` and `ran` pass: no budget missed, and every benchmark
/// ran to its end.
pub fn passed(rows: &[(Budget, Option<f64>, Verdict)], ran: &[Ran]) -> bool {
    rows.iter().all(|r| r.2 != Verdict::Missed) && ran.iter().all(|r| r.ok)
}

fn bound_text(bound: Bound, unit: &str) -> String {
    match bound {
        Bound::AtMost(limit) => format!("≤ {limit} {unit}"),
        Bound::AtLeast(limit) => format!("≥ {limit} {unit}"),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The report, as JSON.
pub fn json(about: &About, ran: &[Ran], measured: &[(String, f64)]) -> Value {
    let budgets: Vec<Value> = rows(measured, about.quick)
        .iter()
        .map(|(b, value, verdict)| {
            let verdict = format!("{verdict:?}").to_lowercase();
            json!({"key": b.key, "what": b.what, "budget": bound_text(b.bound, b.unit),
                   "phase": b.phase, "measured": value, "verdict": verdict})
        })
        .collect();
    let ran: Vec<Value> = ran
        .iter()
        .map(|r| json!({"name": r.name, "ok": r.ok, "seconds": r.seconds}))
        .collect();
    json!({"when": about.when, "commit": about.commit, "machine": about.machine,
           "quick": about.quick, "cuda": about.cuda, "budgets": budgets, "benchmarks": ran})
}

/// The report, as a page of its own.
pub fn html(about: &About, ran: &[Ran], measured: &[(String, f64)]) -> String {
    let mut body = String::new();
    for (b, value, verdict) in rows(measured, about.quick) {
        let shown = value.map_or("—".to_owned(), |v| format!("{v} {}", b.unit));
        let (class, said) = match verdict {
            Verdict::Met => ("met", "met"),
            Verdict::Missed => ("missed", "missed"),
            Verdict::Unmeasured => ("none", "not measured"),
        };
        body.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"{class}\">{said}</td></tr>\n",
            escape(b.what),
            escape(&bound_text(b.bound, b.unit)),
            escape(&shown),
            b.phase,
        ));
    }
    let mut runs = String::new();
    for r in ran {
        let state = if r.ok { "ran" } else { "failed" };
        runs.push_str(&format!(
            "<li>{}: {state}, in {:.0} s</li>\n",
            escape(&r.name),
            r.seconds
        ));
    }
    let quick = match about.quick {
        true => {
            "<p class=\"missed\">Quick runs: shorter than the budgets ask, such as 30 s of recording for 10 minutes, whose length isn't then measured.</p>"
        }
        false => "",
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OScope performance</title>
<style>
:root {{ color-scheme: light dark; font: 15px system-ui, sans-serif; }}
body {{ margin: 0 auto; padding: 12px 16px; max-width: 960px; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ text-align: left; padding: 4px 8px; border-bottom: 1px solid #8884; }}
.met {{ color: #2a8a3e; }} .missed {{ color: #c0392b; }} .none {{ color: #888; }}
</style>
</head>
<body>
<h1>OScope against plan §5.20's budgets</h1>
<p>{when}, commit <code>{commit}</code>, on {machine}; CUDA {cuda}.</p>
{quick}
<table>
<tr><th>What</th><th>Budget</th><th>Measured</th><th>Phase</th><th>Verdict</th></tr>
{body}</table>
<h2>The benchmarks</h2>
<ul>
{runs}</ul>
</body>
</html>
"#,
        when = escape(&about.when),
        commit = escape(&about.commit),
        machine = escape(&about.machine),
        cuda = if about.cuda {
            "built in"
        } else {
            "not built in"
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_lines_are_read_and_held_to_their_budgets() {
        let output =
            "a table\nperf: preview.cpu_ms 34.2\n  perf: zoom.kept_ms 1.5\nperf: odd\nperf: x y\n";
        let measured = measures(output);
        let pairs = [
            ("preview.cpu_ms".to_owned(), 34.2),
            ("zoom.kept_ms".to_owned(), 1.5),
        ];
        assert_eq!(measured, pairs);
        let rows = rows(&measured, false);
        assert_eq!(rows.len(), BUDGETS.len());
        let find = |key: &str| rows.iter().find(|r| r.0.key == key).unwrap().2;
        assert_eq!(find("preview.cpu_ms"), Verdict::Met);
        assert_eq!(find("zoom.kept_ms"), Verdict::Missed);
        assert_eq!(find("preview.gpu_ms"), Verdict::Unmeasured);
        // A key printed twice: the last counts.
        let twice = measures("perf: zoom.kept_ms 2\nperf: zoom.kept_ms 0.5\n");
        assert_eq!(super::rows(&twice, false)[17].2, Verdict::Met);
        // Passing: nothing missed, and every benchmark ran to its end.
        let ok = Ran {
            name: "a".into(),
            ok: true,
            seconds: 1.0,
        };
        let failed = Ran {
            ok: false,
            ..ok.clone()
        };
        assert!(!passed(&rows, std::slice::from_ref(&ok)));
        let met = super::rows(&[("preview.cpu_ms".to_owned(), 1.0)], false);
        assert!(passed(&met, &[ok]) && !passed(&met, &[failed]));
        // A quick run's lengths aren't the budgets': not measured.
        let short = [("record.seconds".to_owned(), 30.0)];
        assert_eq!(super::rows(&short, false)[10].2, Verdict::Missed);
        assert_eq!(super::rows(&short, true)[10].2, Verdict::Unmeasured);
        // Each bound, at its edge.
        let at_least = budget("k", "w", Bound::AtLeast(10.0), "u", "P");
        let at_most = budget("k", "w", Bound::AtMost(10.0), "u", "P");
        assert_eq!(verdict(&at_least, Some(10.0)), Verdict::Met);
        assert_eq!(verdict(&at_least, Some(9.9)), Verdict::Missed);
        assert_eq!(verdict(&at_most, Some(10.0)), Verdict::Met);
        assert_eq!(verdict(&at_most, Some(10.1)), Verdict::Missed);
    }

    #[test]
    fn each_benchmark_runs_quick_or_whole_with_cuda_where_it_has_a_gpu() {
        let demosaic = &BENCHES[1];
        let args = with_cuda((demosaic.args)(true, "s"), demosaic.cuda);
        let line = args.join(" ");
        assert_eq!(
            line,
            "run --release -p ex-demosaic --features cuda -- --smoke --threads 8 --runs 3"
        );
        let record = &BENCHES[2];
        let whole = (record.args)(false, "C:/scratch here").join(" ");
        assert!(whole.contains("--seconds 600") && whole.ends_with("--scratch C:/scratch here"));
        // Every budget's key is one a benchmark prints.
        assert_eq!(BUDGETS.len(), 28);
        assert!(BUDGETS.iter().all(|b| b.key.contains('.')));
    }

    #[test]
    fn the_apps_budgets_are_held_to_and_one_broken_fails_the_run() {
        let output = "perf: web.live.fps 14.5
perf: web.live.latency_ms 40.0
                      perf: web.bundle.largest_kb 259.7
perf: web.memory.mb_per_hour 5
";
        let ok = Ran {
            name: "the app (W10.3)".into(),
            ok: true,
            seconds: 60.0,
        };
        let rows = rows(&measures(output), false);
        let find = |rows: &[(Budget, Option<f64>, Verdict)], key: &str| {
            rows.iter().find(|r| r.0.key == key).unwrap().2
        };
        assert_eq!(find(&rows, "web.live.fps"), Verdict::Met);
        assert_eq!(find(&rows, "web.first_frame_ms"), Verdict::Unmeasured);
        assert!(passed(&rows, std::slice::from_ref(&ok)));
        // A budget broken: the run fails.
        let slow = rows_of(
            "perf: web.live.fps 9.5
",
            false,
        );
        assert_eq!(find(&slow, "web.live.fps"), Verdict::Missed);
        assert!(!passed(&slow, std::slice::from_ref(&ok)));
        let heavy = rows_of(
            "perf: web.bundle.entry_kb 120
",
            false,
        );
        assert!(!passed(&heavy, std::slice::from_ref(&ok)));
        // The memory's growth needs a whole run: a quick one's is noise.
        let leak = "perf: web.memory.mb_per_hour 80
";
        assert!(!passed(&rows_of(leak, false), std::slice::from_ref(&ok)));
        assert!(passed(&rows_of(leak, true), std::slice::from_ref(&ok)));
    }

    fn rows_of(output: &str, quick: bool) -> Vec<(Budget, Option<f64>, Verdict)> {
        rows(&measures(output), quick)
    }

    #[test]
    fn the_report_says_each_verdict_and_how_it_ran() {
        let about = About {
            when: "now".into(),
            commit: "abc".into(),
            machine: "this <one>".into(),
            quick: true,
            cuda: false,
        };
        let ran = [Ran {
            name: "deep zoom (P40)".into(),
            ok: false,
            seconds: 12.0,
        }];
        let measured = [("zoom.cold_ms".to_owned(), 2.0)];
        let page = html(&about, &ran, &measured);
        assert!(page.contains("this &lt;one&gt;") && page.contains("Quick runs"));
        assert!(page.contains("<td>2 ms</td><td>P40</td><td class=\"met\">met</td>"));
        assert!(page.contains("deep zoom (P40): failed, in 12 s"));
        let report = json(&about, &ran, &measured);
        assert_eq!(report["budgets"][16]["verdict"], "met");
        assert_eq!(report["budgets"][0]["verdict"], "unmeasured");
    }
}
