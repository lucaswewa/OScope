//! The recipe library (W7.1, ADR-0052): OScope's stock recipes, which can't
//! be changed, and the user's, kept as `<folder>/<name>.json`, by name.
//!
//! - **A name** is 1 to 64 characters of `a`–`z`, `0`–`9`, `-` and `_`: it
//!   can't leave its folder, or name a stock recipe.
//! - **A recipe kept** is one that reads as a recipe. Whether it can run is
//!   [`check`]'s to say, so that an editor can keep one half made.

use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::plan::Plan;
use crate::recipe::{Problem, Recipe};
use crate::registry::Registry;

/// The stock recipes: name, then their JSON.
const STOCK: [(&str, &str); 8] = [
    ("count-nuclei", include_str!("../recipes/count-nuclei.json")),
    ("false-colour", include_str!("../recipes/false-colour.json")),
    ("green", include_str!("../recipes/green.json")),
    ("inverted", include_str!("../recipes/inverted.json")),
    ("levels", include_str!("../recipes/levels.json")),
    ("smooth", include_str!("../recipes/smooth.json")),
    // The stains, separated (W7.6), and DAB scored.
    ("stains-hdab", include_str!("../recipes/stains-hdab.json")),
    ("stains-he", include_str!("../recipes/stains-he.json")),
];

/// A recipe in the library, as it is listed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Listed {
    /// Its name in the library.
    pub name: String,
    /// The recipe's own name, for people.
    pub title: String,
    /// Whether it is OScope's own, which can't be changed.
    pub stock: bool,
}

/// A recipe in the library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Kept {
    /// Its name in the library.
    pub name: String,
    /// Whether it is OScope's own, which can't be changed.
    pub stock: bool,
    /// The recipe.
    pub recipe: Recipe,
}

/// Why the library refused, or failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LibraryError {
    /// The name, or what was asked of it, isn't allowed.
    #[error("{0}")]
    Refused(String),
    /// There is no recipe of that name.
    #[error("no recipe is named {0:?}")]
    NotFound(String),
    /// Its folder couldn't be read or written.
    #[error("{0}")]
    Io(String),
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> LibraryError + '_ {
    move |e| LibraryError::Io(format!("{}: {e}", path.display()))
}

fn stock(name: &str) -> Option<Recipe> {
    let (_, json) = STOCK.iter().find(|(stock, _)| *stock == name)?;
    Recipe::from_json(json).ok()
}

/// `name`, if a user's recipe may have it.
fn checked(name: &str) -> Result<(), LibraryError> {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    if !((1..=64).contains(&name.len()) && name.chars().all(allowed)) {
        return Err(LibraryError::Refused(format!(
            "a recipe's name has 1 to 64 characters of a–z, 0–9, - and _: {name:?}"
        )));
    }
    if stock(name).is_some() {
        return Err(LibraryError::Refused(format!(
            "{name} is OScope's own, which can't be changed"
        )));
    }
    Ok(())
}

/// The recipes kept in a folder, and the stock ones.
#[derive(Clone, Debug)]
pub struct Library {
    folder: PathBuf,
}

impl Library {
    /// The recipes kept in `folder`, made when one is first kept.
    pub fn open(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.folder.join(format!("{name}.json"))
    }

    /// The recipes, the stock ones first, each by name.
    pub fn list(&self) -> Result<Vec<Listed>, LibraryError> {
        let mut listed: Vec<Listed> = STOCK
            .iter()
            .filter_map(|(name, _)| stock(name).map(|recipe| (*name, recipe)))
            .map(|(name, recipe)| Listed {
                name: name.to_owned(),
                title: recipe.name,
                stock: true,
            })
            .collect();
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return Ok(listed);
        };
        let mut kept = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let json = path.extension().is_some_and(|e| e == "json");
            if !json || checked(name).is_err() {
                continue;
            }
            // One that doesn't read as a recipe is listed by its name alone.
            let title = fs::read_to_string(&path)
                .ok()
                .and_then(|text| Recipe::from_json(&text).ok())
                .map(|recipe| recipe.name)
                .unwrap_or_default();
            kept.push(Listed {
                name: name.to_owned(),
                title,
                stock: false,
            });
        }
        kept.sort_by(|a, b| a.name.cmp(&b.name));
        listed.extend(kept);
        Ok(listed)
    }

    /// Recipe `name`.
    pub fn get(&self, name: &str) -> Result<Kept, LibraryError> {
        if let Some(recipe) = stock(name) {
            return Ok(Kept {
                name: name.to_owned(),
                stock: true,
                recipe,
            });
        }
        checked(name)?;
        let path = self.path(name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(LibraryError::NotFound(name.to_owned()));
            }
            Err(e) => return Err(io(&path)(e)),
        };
        let recipe = Recipe::from_json(&text).map_err(|e| LibraryError::Io(e.to_string()))?;
        Ok(Kept {
            name: name.to_owned(),
            stock: false,
            recipe,
        })
    }

    /// Keeps `recipe` as `name`, in place of one of that name.
    pub fn save(&self, name: &str, recipe: &Recipe) -> Result<(), LibraryError> {
        checked(name)?;
        fs::create_dir_all(&self.folder).map_err(io(&self.folder))?;
        let path = self.path(name);
        // Written whole beside it, then moved into place: never half kept.
        let part = self.folder.join(format!(".{name}.json.part"));
        fs::write(&part, recipe.to_json()).map_err(io(&part))?;
        fs::rename(&part, &path).map_err(io(&path))
    }

    /// Deletes recipe `name`, a user's.
    pub fn delete(&self, name: &str) -> Result<(), LibraryError> {
        checked(name)?;
        let path = self.path(name);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(LibraryError::NotFound(name.to_owned()))
            }
            Err(e) => Err(io(&path)(e)),
        }
    }
}

/// Every fault of the recipe `json`, at once, each with the node and the
/// port it is at where it is at one: none if it can run, here, with a GPU if
/// `gpu` says so. JSON that isn't a recipe is one fault, at `recipe`.
pub fn check(registry: &Registry, json: &serde_json::Value, gpu: bool) -> Vec<Problem> {
    let recipe: Recipe = match serde_json::from_value(json.clone()) {
        Ok(recipe) => recipe,
        Err(error) => return vec![Problem::new("recipe", error.to_string())],
    };
    match Plan::new(registry, &recipe, gpu) {
        Ok(_) => Vec::new(),
        Err(error) => error.problems,
    }
}
