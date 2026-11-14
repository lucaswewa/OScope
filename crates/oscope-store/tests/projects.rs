//! Projects and their records: made, changed, listed, searched and deleted;
//! records drafted, then committed with their thumbnails; the index rebuilt
//! from the folders; and several writers at once.

use std::path::Path;
use std::sync::Arc;

use oscope_core::ProjectId;
use oscope_store::{
    Change, ImageInfo, Input, NewProject, NodeVersion, Provenance, Query, SampleType, Samples,
    Store, StoreError,
};
use serde_json::{Map, json};

fn store(data: &Path) -> Store {
    Store::open(data, &data.join("index.sqlite")).unwrap()
}

fn project(name: &str, sample: &str, tags: &[&str]) -> NewProject {
    NewProject {
        name: name.to_owned(),
        sample: sample.to_owned(),
        tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        owner: "lab".to_owned(),
        metadata: Map::new(),
    }
}

fn names(store: &Store, query: Query) -> (usize, Vec<String>) {
    let page = store.list(&query).unwrap();
    (
        page.total,
        page.projects.into_iter().map(|s| s.project.name).collect(),
    )
}

/// Adds an acquisition to `project`: a gradient, `width` × `height`, RGB.
fn acquire(
    store: &Store,
    project: ProjectId,
    width: u64,
    height: u64,
) -> oscope_store::Acquisition {
    let info = ImageInfo::new(width, height, 3, SampleType::U16);
    let mut draft = store.acquisition(project, "field 1", &info).unwrap();
    let samples: Vec<u16> = (0..height)
        .flat_map(|y| (0..width).flat_map(move |x| [(x * 40) as u16, (y * 40) as u16, 20_000]))
        .collect();
    let image = draft.image().unwrap();
    image
        .write(0, (0, 0), (width, height), &Samples::U16(samples))
        .unwrap();
    draft.record().camera = json!({"exposure": 12.5});
    draft.commit().unwrap()
}

#[test]
fn projects_are_made_changed_listed_searched_and_deleted() {
    let folder = tempfile::tempdir().unwrap();
    let store = store(folder.path());
    let kidney = store
        .create(project("Kidney", "Mouse kidney, H&E", &["he", "mouse"]))
        .unwrap();
    let tonsil = store
        .create(project(
            " Tonsil ",
            "Human tonsil",
            &["he", "he", " human "],
        ))
        .unwrap();
    store
        .create(project("Blood smear", "100%_cells", &[]))
        .unwrap();
    assert_eq!(tonsil.name, "Tonsil");
    assert_eq!(tonsil.tags, ["he", "human"]);
    assert!(
        folder
            .path()
            .join("projects")
            .join(kidney.id.to_string())
            .join("project.json")
            .is_file()
    );

    // The most recently changed first; searched in any case; by whole tags.
    let all = names(&store, Query::default());
    assert_eq!(
        all,
        (
            3,
            vec!["Blood smear".into(), "Tonsil".into(), "Kidney".into()]
        )
    );
    let text = |text: &str| Query {
        text: Some(text.to_owned()),
        ..Query::default()
    };
    assert_eq!(names(&store, text("KIDNEY")).1, ["Kidney"]);
    assert_eq!(names(&store, text("tonsil")).1, ["Tonsil"]);
    assert_eq!(names(&store, text("100%_")).1, ["Blood smear"]);
    assert_eq!(
        names(&store, text("%")).1,
        ["Blood smear"],
        "% is a character, not a wildcard"
    );
    assert_eq!(names(&store, text("_")).1, ["Blood smear"]);
    let tag = |tag: &str| Query {
        tag: Some(tag.to_owned()),
        ..Query::default()
    };
    assert_eq!(names(&store, tag("he")).1, ["Tonsil", "Kidney"]);
    assert_eq!(
        names(&store, tag("h")).1,
        Vec::<String>::new(),
        "tags match whole"
    );
    let page = Query {
        offset: Some(1),
        limit: Some(1),
        ..Query::default()
    };
    assert_eq!(names(&store, page), (3, vec!["Tonsil".into()]));

    // Changes: renamed, retagged, metadata set and removed.
    let metadata = json!({"stain": "H&E", "slide": 7})
        .as_object()
        .unwrap()
        .clone();
    let change = Change {
        name: Some("Kidney 2".into()),
        metadata: Some(metadata),
        ..Change::default()
    };
    let changed = store.change(kidney.id, change).unwrap();
    assert_eq!((changed.name.as_str(), changed.tags.len()), ("Kidney 2", 2));
    assert!(changed.modified > kidney.modified);
    let remove = json!({"slide": null}).as_object().unwrap().clone();
    let retag = Change {
        tags: Some(vec!["kidney".into()]),
        metadata: Some(remove),
        ..Change::default()
    };
    let changed = store.change(kidney.id, retag).unwrap();
    assert_eq!(
        changed.metadata,
        *json!({"stain": "H&E"}).as_object().unwrap()
    );
    assert_eq!(
        store.project(kidney.id).unwrap(),
        changed,
        "as the folder has it"
    );
    assert_eq!(names(&store, Query::default()).1[0], "Kidney 2");

    // Refused, and not found.
    let refused = |result: Result<_, StoreError>| match result {
        Err(StoreError::Refused(why)) => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        refused(store.create(project("  ", "", &[]))),
        "a name has 1 to 200 characters: \"\""
    );
    let long = Change {
        tags: Some(vec!["x".repeat(65)]),
        ..Change::default()
    };
    assert!(refused(store.change(tonsil.id, long)).starts_with("a tag has 1 to 64 characters"));
    let nowhere = ProjectId::generate();
    let missing = store
        .change(nowhere, Change::default())
        .unwrap_err()
        .to_string();
    assert_eq!(missing, format!("no project {nowhere}"));

    store.delete(tonsil.id).unwrap();
    assert!(
        !folder
            .path()
            .join("projects")
            .join(tonsil.id.to_string())
            .exists()
    );
    assert_eq!(names(&store, Query::default()).0, 2);
    assert!(matches!(
        store.delete(tonsil.id),
        Err(StoreError::NotFound { .. })
    ));
}

#[test]
fn records_are_drafted_then_committed_with_their_thumbnails() {
    let folder = tempfile::tempdir().unwrap();
    let store = store(folder.path());
    let slide = store.create(project("Slide", "", &[])).unwrap();

    // Until it is committed, an acquisition isn't listed; dropped, it goes.
    let info = ImageInfo::new(64, 64, 1, SampleType::U8);
    let draft = store.acquisition(slide.id, "abandoned", &info).unwrap();
    let abandoned = draft.folder().to_owned();
    assert!(abandoned.is_dir());
    assert_eq!(store.contents(slide.id).unwrap().acquisitions, []);
    drop(draft);
    assert!(!abandoned.exists());

    let acquisition = acquire(&store, slide.id, 1_200, 700);
    assert_eq!(
        (
            acquisition.image.levels,
            acquisition.camera["exposure"].as_f64()
        ),
        (3, Some(12.5))
    );
    let place = store
        .folder(slide.id)
        .join("acquisitions")
        .join(acquisition.id.to_string());
    for file in [
        "acquisition.json",
        "raw.ome.zarr/zarr.json",
        "thumbnail.jpg",
    ] {
        assert!(place.join(file).is_file(), "{file}");
    }
    let thumbnail = &oscope_imaging::color::read_image(&place.join("thumbnail.jpg")).unwrap()[0];
    assert_eq!(
        (thumbnail.width(), thumbnail.height()),
        (200, 117),
        "level 1, 600 × 350, by 3"
    );

    // A result made from it, with its provenance, and files of its own.
    let provenance = Provenance {
        recipe: json!({"format": 1, "name": "smooth"}),
        nodes: vec![NodeVersion {
            node: "filter.gaussian".into(),
            version: "1".into(),
            extension: None,
        }],
        inputs: vec![Input::Acquisition(acquisition.id)],
        software: "OScope 0.1.0".into(),
    };
    let draft = store
        .result(slide.id, "counts", provenance.clone(), None)
        .unwrap();
    std::fs::write(draft.folder().join("counts.csv"), "cells\n42\n").unwrap();
    let result = draft.commit().unwrap();
    let mut report = store.report(slide.id, "summary").unwrap();
    std::fs::write(report.folder().join("report.html"), "<p>42 cells</p>").unwrap();
    report.record().files.push("report.html".into());
    let report = report.commit().unwrap();

    let contents = store.contents(slide.id).unwrap();
    assert_eq!(contents.acquisitions, std::slice::from_ref(&acquisition));
    assert_eq!(contents.results, std::slice::from_ref(&result));
    assert_eq!(contents.reports, [report]);
    assert_eq!(contents.results[0].provenance, provenance);
    assert!(
        contents.project.modified > slide.modified,
        "a commit changes the project"
    );
    let listed = &store.list(&Query::default()).unwrap().projects[0];
    assert_eq!(
        (listed.acquisitions, listed.results, listed.reports),
        (1, 1, 1)
    );
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(place.join("acquisition.json")).unwrap()).unwrap();
    assert_eq!(
        (written["schema"].as_str(), written["version"].as_u64()),
        (Some("oscope.acquisition"), Some(1))
    );

    // The project's thumbnail is its newest acquisition's, even with a newer
    // image result; a record's, its own; a record without an image has none.
    let grey = ImageInfo::new(64, 48, 1, SampleType::F32);
    let draft = store
        .result(slide.id, "smoothed", provenance, Some(&grey))
        .unwrap();
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (64, 48), &Samples::F32(vec![0.5; 64 * 48]))
        .unwrap();
    let smoothed = draft.commit().unwrap();
    let cover = store.thumbnail(slide.id, None).unwrap();
    assert_eq!(cover, place.join("thumbnail.jpg"));
    assert_eq!(
        store
            .thumbnail(slide.id, Some(&acquisition.id.to_string()))
            .unwrap(),
        cover
    );
    let own = store
        .thumbnail(slide.id, Some(&smoothed.id.to_string()))
        .unwrap();
    assert!(
        own.ends_with(format!("results/{}/thumbnail.jpg", smoothed.id)),
        "{own:?}"
    );
    assert!(
        store
            .thumbnail(slide.id, Some(&result.id.to_string()))
            .is_err()
    );
    let outside = store
        .thumbnail(slide.id, Some("../../x"))
        .unwrap_err()
        .to_string();
    assert_eq!(outside, "\"../../x\" isn't a record's ID");
    // Without acquisitions, the newest image result's.
    let analysis = store.create(project("Analysis", "", &[])).unwrap();
    let draft = store
        .result(
            analysis.id,
            "map",
            Provenance {
                software: "x".into(),
                ..result.provenance.clone()
            },
            Some(&grey),
        )
        .unwrap();
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (64, 48), &Samples::F32(vec![0.25; 64 * 48]))
        .unwrap();
    let map = draft.commit().unwrap();
    assert!(
        store
            .thumbnail(analysis.id, None)
            .unwrap()
            .ends_with(format!("results/{}/thumbnail.jpg", map.id))
    );
    let empty = store.create(project("Empty", "", &[])).unwrap();
    assert!(matches!(
        store.thumbnail(empty.id, None),
        Err(StoreError::NotFound { .. })
    ));
    // No record goes into a project that isn't there.
    let gone = ProjectId::generate();
    assert!(matches!(
        store.report(gone, "x"),
        Err(StoreError::NotFound { .. })
    ));
}

#[test]
fn a_rebuilt_index_equals_the_live_one() {
    let folder = tempfile::tempdir().unwrap();
    let live = store(folder.path());
    for n in 0..4 {
        let made = live
            .create(project(&format!("Project {n}"), "sample", &["a", "b"]))
            .unwrap();
        acquire(&live, made.id, 300, 200);
        let provenance = Provenance {
            software: "OScope".into(),
            recipe: json!(null),
            nodes: vec![],
            inputs: vec![],
        };
        live.result(made.id, "result", provenance, None)
            .unwrap()
            .commit()
            .unwrap();
        live.report(made.id, "report").unwrap().commit().unwrap();
    }
    let rows = live.index_rows().unwrap();
    assert_eq!(rows.len(), 4 + 4 * 3);
    // An unfinished record, and a project left half deleted: neither counts.
    let first = live.list(&Query::default()).unwrap().projects[0].project.id;
    let _unfinished = live
        .acquisition(
            first,
            "unfinished",
            &ImageInfo::new(8, 8, 1, SampleType::U8),
        )
        .unwrap();
    std::fs::create_dir_all(live.root().join(".deleting-x/acquisitions")).unwrap();

    // From the folders alone, into an index that is lost.
    let elsewhere = tempfile::tempdir().unwrap();
    let rebuilt = Store::open(folder.path(), &elsewhere.path().join("index.sqlite")).unwrap();
    assert_eq!(rebuilt.index_rows().unwrap(), Vec::<String>::new());
    rebuilt.rebuild_index().unwrap();
    assert_eq!(rebuilt.index_rows().unwrap(), rows);
    assert!(!live.root().join(".deleting-x").exists());
    // And in place.
    live.rebuild_index().unwrap();
    assert_eq!(live.index_rows().unwrap(), rows);
    assert_eq!(
        rebuilt.list(&Query::default()).unwrap(),
        live.list(&Query::default()).unwrap()
    );
}

#[test]
fn writers_at_once_lose_nothing() {
    // Eight writers, each with a store and an index connection of its own,
    // as other processes would have, on the same folders and index.
    let folder = tempfile::tempdir().unwrap();
    let shared = store(folder.path())
        .create(project("Shared", "", &[]))
        .unwrap();
    let data = Arc::new(folder.path().to_owned());
    let writers: Vec<_> = (0..8)
        .map(|w| {
            let data = Arc::clone(&data);
            std::thread::spawn(move || {
                let store = store(&data);
                for n in 0..3 {
                    store
                        .create(project(&format!("w{w} p{n}"), "", &[]))
                        .unwrap();
                    // Each writer's key, into the same project's metadata.
                    let key = json!({format!("w{w} n{n}"): n})
                        .as_object()
                        .unwrap()
                        .clone();
                    store
                        .change(
                            shared.id,
                            Change {
                                metadata: Some(key),
                                ..Change::default()
                            },
                        )
                        .unwrap();
                }
                acquire(&store, shared.id, 96, 64);
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    let store = store(folder.path());
    assert_eq!(store.list(&Query::default()).unwrap().total, 1 + 8 * 3);
    let contents = store.contents(shared.id).unwrap();
    assert_eq!(contents.project.metadata.len(), 8 * 3, "no change lost");
    assert_eq!(contents.acquisitions.len(), 8);
    let rows = store.index_rows().unwrap();
    store.rebuild_index().unwrap();
    assert_eq!(store.index_rows().unwrap(), rows);
}

#[test]
fn thumbnails_stretch_dim_images_and_show_8_bit_ones_as_they_are() {
    let folder = tempfile::tempdir().unwrap();
    let store = store(folder.path());
    let slide = store.create(project("Slide", "", &[])).unwrap();
    // A dim 16-bit ramp, 1,000 to 3,000 of 65,535: stretched to fill the
    // display. An 8-bit ramp of the same levels in 8 bits: shown as it is.
    let ramp = |n: u64| (0..64).flat_map(move |_| (0..n).map(move |x| x * 2_000 / (n - 1) + 1_000));
    let lightest = |image: oscope_store::Samples, sample| {
        let info = ImageInfo::new(256, 64, 1, sample);
        let draft = store.acquisition(slide.id, "ramp", &info).unwrap();
        draft
            .image()
            .unwrap()
            .write(0, (0, 0), (256, 64), &image)
            .unwrap();
        let made = draft.commit().unwrap();
        let path = store
            .thumbnail(slide.id, Some(&made.id.to_string()))
            .unwrap();
        let read = &oscope_imaging::color::read_image(&path).unwrap()[0];
        read.data().iter().copied().fold(0.0_f32, f32::max)
    };
    let dim = lightest(
        Samples::U16(ramp(256).map(|v| v as u16).collect()),
        SampleType::U16,
    );
    assert!(dim > 0.9, "stretched: {dim}");
    let eight = lightest(
        Samples::U8(ramp(256).map(|v| (v / 256) as u8).collect()),
        SampleType::U8,
    );
    assert!(eight < 0.01, "as it is: {eight}");
}

#[test]
fn a_page_holds_at_most_500_projects() {
    let folder = tempfile::tempdir().unwrap();
    let store = store(folder.path());
    for n in 0..501 {
        store.create(project(&format!("p{n}"), "", &[])).unwrap();
    }
    let page = store
        .list(&Query {
            limit: Some(10_000),
            ..Query::default()
        })
        .unwrap();
    assert_eq!((page.total, page.projects.len()), (501, 500));
    let rest = store
        .list(&Query {
            offset: Some(500),
            ..Query::default()
        })
        .unwrap();
    assert_eq!(rest.projects.len(), 1);
}
