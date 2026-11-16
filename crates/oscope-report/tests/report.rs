//! Reports (P47): the sample report rendered to PDF, its pages' text; a
//! long one over pages; its HTML, as a snapshot; templates' errors, where
//! they are; the sandbox they compile in; the charts; and templates kept,
//! checked and removed.

use oscope_report::{
    Block, Chart, ChartKind, ReportError, STANDARD, Templates, html, pdf, sample, svg,
};

#[test]
fn the_sample_renders_to_pdf_with_its_text() {
    let (report, images) = sample();
    let standard = Templates::open(std::path::Path::new("nowhere"))
        .get(STANDARD)
        .unwrap();
    let rendered = pdf(&standard.typst, &report, &images).unwrap();
    assert!(rendered.bytes.starts_with(b"%PDF-"), "a PDF");
    assert_eq!(rendered.pages.len(), 1);
    let text = &rendered.pages[0];
    for want in [
        "A sample report",
        "Every kind of block",
        "2026-01-01",
        "Project",
        "a slide",
        "Results",
        "Some text.",
        "An image",
        "A table",
        "A chart",
        "Provenance",
        "OScope",
        "1 / 1",
    ] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    // A long table runs over pages, numbered.
    let (mut long, images) = sample();
    let rows = (0..200)
        .map(|k| vec![k.to_string(), (k * k).to_string()])
        .collect();
    long.sections[0].blocks.push(Block::Table {
        caption: "A long table".to_owned(),
        columns: vec!["k".to_owned(), "k²".to_owned()],
        rows,
    });
    let rendered = pdf(&standard.typst, &long, &images).unwrap();
    let pages = rendered.pages.len();
    assert!(pages >= 3, "{pages} pages");
    assert!(rendered.pages[pages - 1].contains(&format!("{pages} / {pages}")));
    assert!(
        rendered.pages.iter().any(|p| p.contains("39601")),
        "the last row, 199²"
    );
}

#[test]
fn the_sample_renders_to_html() {
    let (report, images) = sample();
    let standard = Templates::open(std::path::Path::new("nowhere"))
        .get(STANDARD)
        .unwrap();
    let page = html(&standard.html, &report, &images).unwrap();
    assert!(
        page.contains("<img src=\"data:image/png;base64,iVBORw0KGgo"),
        "the image inline"
    );
    assert!(
        page.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "the chart inline"
    );
    insta::assert_snapshot!("sample", page);
    // Text is escaped.
    let (mut odd, images) = sample();
    odd.title = "<b>bold</b> & co".to_owned();
    let page = html(&standard.html, &odd, &images).unwrap();
    assert!(
        page.contains("&lt;b&gt;bold") && page.contains("&amp; co") && !page.contains("<b>bold"),
        "escaped"
    );
}

#[test]
fn a_template_s_errors_say_where_they_are() {
    let (report, images) = sample();
    let wrong = "#let report = json(\"/report.json\")\n= #report.title\n#report.no_such_field\n";
    match pdf(wrong, &report, &images) {
        Err(ReportError::Template {
            kind: "typst",
            line: 3,
            column,
            message,
        }) => {
            assert!(
                column > 0 && message.contains("no_such_field"),
                "{column} {message}"
            );
        }
        other => panic!("{other:?}"),
    }
    let unclosed = "#let report = json(\"/report.json\")\n#[\n";
    assert!(matches!(
        pdf(unclosed, &report, &images),
        Err(ReportError::Template {
            kind: "typst",
            line: 2,
            ..
        })
    ));
    let missing = "#image(\"/images/none.png\")";
    assert!(matches!(
        pdf(missing, &report, &images),
        Err(ReportError::Template {
            kind: "typst",
            line: 1,
            ..
        })
    ));
    let html_wrong = "<p>{{ report.title }}</p>\n{% for x in %}\n";
    match html(html_wrong, &report, &images) {
        Err(ReportError::Template {
            kind: "html",
            line: 2,
            ..
        }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_sandbox_holds_the_report_alone() {
    let (report, images) = sample();
    // Today is the report's date, so a PDF depends on its model alone.
    let dated = "#datetime.today().display(\"[day].[month].[year]\")";
    let rendered = pdf(dated, &report, &images).unwrap();
    assert!(
        rendered.pages[0].contains("01.01.2026"),
        "{:?}",
        rendered.pages
    );
    // Its own files are read, and nothing else: no file of the disk's, no
    // other source, no package.
    let own = "#read(\"/report.json\").len()";
    assert!(pdf(own, &report, &images).is_ok());
    for outside in [
        "#read(\"/../Cargo.toml\")",
        "#read(\"/etc/hosts\")",
        "#include \"other.typ\"",
        "#import \"@preview/example:0.1.0\": *",
    ] {
        let refused = pdf(outside, &report, &images);
        assert!(
            matches!(
                refused,
                Err(ReportError::Template {
                    kind: "typst",
                    line: 1,
                    ..
                })
            ),
            "{outside}: {refused:?}"
        );
    }
}

#[test]
fn charts_are_drawn_to_scale() {
    let chart = Chart {
        kind: ChartKind::Bars,
        labels: vec!["a".to_owned(), "b<c".to_owned(), "c".to_owned()],
        values: vec![1.0, 4.0, 2.0],
        x_label: "x".to_owned(),
        y_label: "y <&>".to_owned(),
    };
    let bars = svg(&chart);
    assert!(bars.starts_with("<svg") && bars.ends_with("</svg>"));
    assert_eq!(bars.matches("<rect").count(), 3);
    // The axis tops at 5, the tallest bar at 4 of it: heights 1 : 4 : 2.
    let heights: Vec<f64> = bars
        .match_indices("height=\"")
        .skip(1)
        .map(|(i, _)| bars[i + 8..].split('"').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(heights.len(), 3);
    assert!(
        (heights[1] / heights[0] - 4.0).abs() < 1e-2
            && (heights[2] / heights[0] - 2.0).abs() < 1e-2,
        "{heights:?}"
    );
    assert!(bars.contains(">5</text>") && bars.contains("y &lt;&amp;&gt;"));
    assert!(bars.contains(">b&lt;c</text>"), "the labels escaped");
    let line = svg(&Chart {
        kind: ChartKind::Line,
        ..chart
    });
    assert_eq!(
        (
            line.matches("<polyline").count(),
            line.matches("<rect").count()
        ),
        (1, 0)
    );
    // The line's points are the bars' tops.
    let tops: Vec<&str> = bars
        .match_indices("<rect")
        .map(|(i, _)| {
            bars[i..]
                .split("y=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
        })
        .collect();
    let points = line
        .split("points=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let ys: Vec<&str> = points
        .split(' ')
        .map(|p| p.split(',').nth(1).unwrap())
        .collect();
    assert_eq!(ys, tops);
}

#[test]
fn templates_are_kept_when_they_render_and_removed() {
    let folder = tempfile::tempdir().unwrap();
    let templates = Templates::open(folder.path());
    assert_eq!(templates.names().unwrap(), [STANDARD]);
    let standard = templates.get(STANDARD).unwrap();
    assert!(standard.builtin);
    // A template of its own: the standard one, retitled.
    let typst = standard
        .typst
        .replace("weight: \"bold\"", "weight: \"regular\"");
    templates.save("plain", &typst, &standard.html).unwrap();
    assert_eq!(templates.names().unwrap(), [STANDARD, "plain"]);
    assert_eq!(templates.get("plain").unwrap().typst, typst);
    // A folder of half a template isn't one.
    std::fs::create_dir_all(folder.path().join("half")).unwrap();
    std::fs::write(folder.path().join("half/report.typ"), &typst).unwrap();
    assert_eq!(templates.names().unwrap(), [STANDARD, "plain"]);
    // One that doesn't render isn't kept, and says why.
    let broken = templates.save("broken", "#[", &standard.html);
    assert!(
        matches!(broken, Err(ReportError::Template { kind: "typst", .. })),
        "{broken:?}"
    );
    let broken = templates.save("broken", &typst, "{% if %}");
    assert!(
        matches!(broken, Err(ReportError::Template { kind: "html", .. })),
        "{broken:?}"
    );
    assert_eq!(templates.names().unwrap(), [STANDARD, "plain"]);
    // Names, the built-in one, and removal.
    for name in ["", "Plain", "a/b", "..", STANDARD, &"x".repeat(65)] {
        assert!(
            matches!(
                templates.save(name, &typst, &standard.html),
                Err(ReportError::Refused(_))
            ),
            "{name:?}"
        );
    }
    assert!(matches!(
        templates.delete(STANDARD),
        Err(ReportError::Refused(_))
    ));
    templates.delete("plain").unwrap();
    assert!(matches!(
        templates.delete("plain"),
        Err(ReportError::Refused(_))
    ));
    assert_eq!(templates.names().unwrap(), [STANDARD]);
}
