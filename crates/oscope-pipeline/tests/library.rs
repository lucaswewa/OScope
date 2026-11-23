//! The recipe library and `check` (W7.1, ADR-0052): the stock recipes, each
//! one that can run; a user's recipe kept, listed, read and deleted; names
//! that would leave the library's folder, or change a stock recipe,
//! refused; and `check` finding a cycle, a type mismatch and a missing
//! input together, each at its node and port.

use oscope_pipeline::{Library, LibraryError, Problem, Recipe, check, stock};
use serde_json::json;

fn recipe(name: &str) -> Recipe {
    Recipe::from_json(
        &json!({
            "format": 1,
            "name": name,
            "inputs": [{ "name": "image", "type": "image" }],
            "nodes": [{ "id": "blur", "node": "filter.gaussian", "params": { "sigma": 3.0 } }],
            "edges": [{ "from": "input.image", "to": "blur.image" }],
            "outputs": [{ "name": "image", "from": "blur.image" }]
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn the_stock_recipes_are_listed_first_and_each_can_run() {
    let folder = tempfile::tempdir().unwrap();
    let registry = stock::registry(folder.path().join("references"));
    let library = Library::open(&folder.path().join("recipes"));
    let listed = library.list().unwrap();
    let names: Vec<&str> = listed.iter().map(|one| one.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "count-nuclei",
            "false-colour",
            "green",
            "inverted",
            "levels",
            "smooth",
            "stains-hdab",
            "stains-he"
        ]
    );
    for one in &listed {
        assert!(one.stock && !one.title.is_empty(), "{one:?}");
        let kept = library.get(&one.name).unwrap();
        assert!(kept.stock);
        let json = serde_json::to_value(&kept.recipe).unwrap();
        // The references a flat field reads aren't in this test's folder: none of these reads one.
        assert_eq!(
            check(&registry, &json, false),
            Vec::<Problem>::new(),
            "{}",
            one.name
        );
    }
}

#[test]
fn a_users_recipe_is_kept_listed_read_and_deleted() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::open(&folder.path().join("recipes"));
    let mine = recipe("Blurred more");
    library.save("blur-3", &mine).unwrap();
    library.save("another", &recipe("Another")).unwrap();
    let listed = library.list().unwrap();
    let users: Vec<(&str, &str, bool)> = listed
        .iter()
        .filter(|one| !one.stock)
        .map(|one| (one.name.as_str(), one.title.as_str(), one.stock))
        .collect();
    assert_eq!(
        users,
        [
            ("another", "Another", false),
            ("blur-3", "Blurred more", false)
        ]
    );
    let kept = library.get("blur-3").unwrap();
    assert!(!kept.stock);
    assert_eq!(kept.recipe, mine);
    // Kept again, in place of the old, nothing half written left beside it.
    library.save("blur-3", &recipe("Blurred again")).unwrap();
    assert_eq!(library.get("blur-3").unwrap().recipe.name, "Blurred again");
    let files = std::fs::read_dir(folder.path().join("recipes"))
        .unwrap()
        .count();
    assert_eq!(files, 2);
    library.delete("blur-3").unwrap();
    assert!(matches!(
        library.get("blur-3"),
        Err(LibraryError::NotFound(_))
    ));
    assert!(matches!(
        library.delete("blur-3"),
        Err(LibraryError::NotFound(_))
    ));
}

#[test]
fn names_that_would_leave_the_folder_or_change_a_stock_recipe_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::open(&folder.path().join("recipes"));
    let long = "a".repeat(65);
    for name in [
        "",
        "../out",
        "..",
        "a/b",
        "a\\b",
        ".hidden",
        "C:out",
        "Capital",
        "with space",
        &long,
    ] {
        let saved = library.save(name, &recipe("out"));
        assert!(matches!(saved, Err(LibraryError::Refused(_))), "{name:?}");
        assert!(
            matches!(library.get(name), Err(LibraryError::Refused(_))),
            "{name:?}"
        );
        assert!(
            matches!(library.delete(name), Err(LibraryError::Refused(_))),
            "{name:?}"
        );
    }
    for name in ["levels", "count-nuclei"] {
        assert!(matches!(
            library.save(name, &recipe("mine")),
            Err(LibraryError::Refused(_))
        ));
        assert!(matches!(
            library.delete(name),
            Err(LibraryError::Refused(_))
        ));
        assert!(library.get(name).unwrap().stock);
    }
    // Nothing was written, inside the folder or out of it.
    assert!(!folder.path().join("recipes").exists());
    assert!(!folder.path().join("out.json").exists());
    assert!(library.list().unwrap().iter().all(|one| one.stock));
}

#[test]
fn a_file_that_isnt_a_recipe_is_listed_by_its_name_and_not_read() {
    let folder = tempfile::tempdir().unwrap();
    let recipes = folder.path().join("recipes");
    std::fs::create_dir_all(&recipes).unwrap();
    std::fs::write(recipes.join("broken.json"), "{ not json").unwrap();
    std::fs::write(recipes.join("notes.txt"), "not a recipe at all").unwrap();
    let library = Library::open(&recipes);
    let users: Vec<_> = library
        .list()
        .unwrap()
        .into_iter()
        .filter(|one| !one.stock)
        .collect();
    assert_eq!(users.len(), 1);
    assert_eq!(
        (users[0].name.as_str(), users[0].title.as_str()),
        ("broken", "")
    );
    assert!(matches!(library.get("broken"), Err(LibraryError::Io(_))));
}

#[test]
fn check_finds_a_cycle_a_type_mismatch_and_a_missing_input_together() {
    let folder = tempfile::tempdir().unwrap();
    let registry = stock::registry(folder.path());
    let broken = json!({
        "format": 1,
        "name": "Broken three ways",
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [
            { "id": "a", "node": "filter.gaussian", "params": { "sigma": 1.0 } },
            { "id": "b", "node": "filter.gaussian", "params": { "sigma": 1.0 } },
            { "id": "level", "node": "threshold.otsu" },
            { "id": "open", "node": "morphology.open", "params": { "radius": 1 } }
        ],
        "edges": [
            { "from": "a.image", "to": "b.image" },
            { "from": "b.image", "to": "a.image" },
            { "from": "input.image", "to": "open.mask" }
        ],
        "outputs": [{ "name": "mask", "from": "level.mask" }]
    });
    let faults = check(&registry, &broken, false);
    let at = |node: &str, port: Option<&str>| {
        faults
            .iter()
            .filter(|f| f.node.as_deref() == Some(node) && f.port.as_deref() == port)
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
    };
    // The cycle, once, at a node of it, naming both.
    let cycle: Vec<_> = faults
        .iter()
        .filter(|f| f.message.contains("cycle"))
        .collect();
    assert_eq!(cycle.len(), 1, "{faults:?}");
    assert!(
        matches!(cycle[0].node.as_deref(), Some("a" | "b")),
        "{faults:?}"
    );
    assert!(cycle[0].message.contains('a') && cycle[0].message.contains('b'));
    // The mismatch, at the input it goes to.
    let mismatch = at("open", Some("mask"));
    assert!(
        mismatch
            .iter()
            .any(|m| m.contains("gives image") && m.contains("takes mask")),
        "{faults:?}"
    );
    // The missing input, at its port.
    assert_eq!(
        at("level", Some("image")),
        ["its input `image` has no edge"],
        "{faults:?}"
    );
    assert_eq!(faults.len(), 3, "{faults:?}");
}

#[test]
fn check_finds_a_bad_parameter_at_its_node_and_json_that_isnt_a_recipe_at_the_recipe() {
    let folder = tempfile::tempdir().unwrap();
    let registry = stock::registry(folder.path());
    let mut json = serde_json::to_value(recipe("Too blurred")).unwrap();
    json["nodes"][0]["params"]["sigma"] = json!(99.0);
    json["edges"][0]["from"] = json!("input.nothing");
    let faults = check(&registry, &json, false);
    assert!(
        faults
            .iter()
            .any(|f| f.node.as_deref() == Some("blur") && f.at.contains("params/sigma")),
        "{faults:?}"
    );
    assert!(
        faults
            .iter()
            .any(|f| f.node.as_deref() == Some("input") && f.port.as_deref() == Some("nothing")),
        "{faults:?}"
    );
    let faults = check(&registry, &json!({ "format": 1, "nodes": "none" }), false);
    assert_eq!(faults.len(), 1);
    assert_eq!(
        (faults[0].at.as_str(), faults[0].node.as_deref()),
        ("recipe", None)
    );
}

#[test]
fn a_recipes_layout_is_kept_and_the_engine_ignores_it() {
    // W7.2: where the editor drew each node.
    let folder = tempfile::tempdir().unwrap();
    let registry = stock::registry(folder.path());
    let library = Library::open(&folder.path().join("recipes"));
    let mut json = serde_json::to_value(recipe("Laid out")).unwrap();
    json["layout"] = json!({ "input": [0.0, 80.0], "blur": [240.0, 80.5], "nothing": [1.0, 2.0] });
    let laid: Recipe = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(laid.layout["blur"], [240.0, 80.5]);
    // Its JSON says it; a recipe without one says nothing of it.
    assert_eq!(
        serde_json::to_value(&laid).unwrap()["layout"],
        json["layout"]
    );
    assert!(
        serde_json::to_value(recipe("plain"))
            .unwrap()
            .get("layout")
            .is_none()
    );
    // Kept, and read back; and no fault for it, a node not in the recipe among it.
    library.save("laid-out", &laid).unwrap();
    assert_eq!(library.get("laid-out").unwrap().recipe, laid);
    assert_eq!(check(&registry, &json, false), Vec::<Problem>::new());
}
