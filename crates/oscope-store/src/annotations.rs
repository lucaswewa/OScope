//! Annotations (P44, plan §5.17): points, lines and polygons drawn on an
//! acquisition's or a result's image, in layers, kept in the record's
//! folder as `annotations.json`.
//!
//! Positions are `[x, y]`, or `[x, y, z]` in a volume, in µm from the
//! image's top-left corner, x across and y down: its pixel size scales its
//! pixels to them, and on an image of unknown pixel size a µm is a pixel.
//! Each layer is a GeoJSON FeatureCollection (RFC 7946), and each feature
//! has an ID unique in its record, given when it is first kept.

use std::collections::BTreeMap;
use std::path::PathBuf;

use oscope_core::{ProjectId, Versioned};
use oscope_imaging::shapes::{self, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::records::RecordKind;
use crate::store::{read, write};
use crate::{Result, Store, StoreError};

/// A shape, in µm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "coordinates")]
pub enum Geometry {
    /// A point: `[x, y]`, or `[x, y, z]` in a volume.
    Point(Vec<f64>),
    /// A line through two points or more.
    LineString(Vec<Vec<f64>>),
    /// An area: its outer ring, then its holes, each of four positions or
    /// more, its last the same as its first.
    Polygon(Vec<Vec<Vec<f64>>>),
}

impl Geometry {
    /// Its type's name, as GeoJSON says it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Point(_) => "Point",
            Self::LineString(_) => "LineString",
            Self::Polygon(_) => "Polygon",
        }
    }
}

/// Positions as points in the plane, their z left out.
pub(crate) fn plane(positions: &[Vec<f64>]) -> Vec<Point> {
    positions.iter().map(|p| [p[0], p[1]]).collect()
}

/// A shape, with its ID and its properties, which `name` names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename = "Feature")]
pub struct Feature {
    /// Its ID, unique in its record: given when it is first kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Its shape.
    pub geometry: Geometry,
    /// What is said of it: its `name`, for one.
    #[serde(default)]
    pub properties: Map<String, Value>,
}

/// A layer of annotations: a GeoJSON FeatureCollection.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename = "FeatureCollection")]
pub struct Layer {
    /// Its features, in the order they were made.
    pub features: Vec<Feature>,
}

/// A record's annotations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Annotations {
    /// Its layers, by name.
    pub layers: BTreeMap<String, Layer>,
    /// The ID its last new feature was given: the next is one more.
    #[serde(default)]
    pub last_id: u64,
}

impl Versioned for Annotations {
    const SCHEMA: &'static str = "oscope.annotations";
    const VERSION: u32 = 1;
}

/// `geometry`, or why it isn't kept.
fn checked(geometry: &Geometry) -> Result<()> {
    let position = |p: &Vec<f64>| (2..=3).contains(&p.len()) && p.iter().all(|v| v.is_finite());
    let (fine, rule) = match geometry {
        Geometry::Point(p) => (position(p), "a point is [x, y] or [x, y, z]"),
        Geometry::LineString(line) => (
            line.len() >= 2 && line.iter().all(position),
            "a line has two points or more",
        ),
        Geometry::Polygon(rings) => {
            let closed = |ring: &Vec<Vec<f64>>| ring.len() >= 4 && ring.first() == ring.last();
            let fine = !rings.is_empty()
                && rings.iter().all(|r| closed(r) && r.iter().all(position))
                && shapes::polygon_area(&rings.iter().map(|r| plane(r)).collect::<Vec<_>>()) > 0.0;
            let rule = "a polygon's rings have four positions or more, their last their first, \
                        and enclose something";
            (fine, rule)
        }
    };
    if fine {
        Ok(())
    } else {
        Err(StoreError::Refused(format!(
            "{rule}, in finite µm: {}",
            serde_json::to_string(geometry).unwrap_or_default()
        )))
    }
}

/// `name`, if a layer may have it: 1 to 64 characters, not starting or
/// ending with a space, none a control.
fn checked_layer(name: &str) -> Result<String> {
    let count = name.chars().count();
    if (1..=64).contains(&count) && name.trim() == name && !name.chars().any(char::is_control) {
        Ok(name.to_owned())
    } else {
        Err(StoreError::Refused(format!(
            "a layer's name has 1 to 64 characters, without spaces at its ends: {name:?}"
        )))
    }
}

impl Store {
    /// The file of `kind` record `id`'s annotations, which needn't exist
    /// yet: an acquisition's or a result's.
    fn annotations_file(&self, project: ProjectId, kind: RecordKind, id: &str) -> Result<PathBuf> {
        if kind == RecordKind::Report || uuid::Uuid::try_parse(id).is_err() {
            return Err(StoreError::Refused(format!(
                "an acquisition or a result is annotated, by its ID: not {} {id:?}",
                kind.name()
            )));
        }
        let folder = self.folder(project).join(kind.folder()).join(id);
        if !folder.is_dir() {
            let id = id.to_owned();
            return Err(StoreError::NotFound {
                kind: kind.name(),
                id,
            });
        }
        Ok(folder.join("annotations.json"))
    }

    /// `kind` record `id`'s annotations, in project `project`: none until
    /// some are kept.
    pub fn annotations(
        &self,
        project: ProjectId,
        kind: RecordKind,
        id: &str,
    ) -> Result<Annotations> {
        let file = self.annotations_file(project, kind, id)?;
        match file.exists() {
            true => read(&file),
            false => Ok(Annotations::default()),
        }
    }

    /// Changes `kind` record `id`'s annotations by `change`, under the
    /// store's lock.
    fn edit<T>(
        &self,
        (project, kind, id): (ProjectId, RecordKind, &str),
        change: impl FnOnce(&mut Annotations) -> Result<T>,
    ) -> Result<T> {
        let file = self.annotations_file(project, kind, id)?;
        let _lock = self.lock()?;
        let mut annotations = match file.exists() {
            true => read(&file)?,
            false => Annotations::default(),
        };
        let done = change(&mut annotations)?;
        write(&file, &annotations)?;
        Ok(done)
    }

    /// Keeps `feature` in layer `layer` of `kind` record `id`, which is made
    /// if need be: in place of the feature with its ID, or with none, as a
    /// new feature with a new ID. The feature as kept.
    pub fn annotate(
        &self,
        project: ProjectId,
        kind: RecordKind,
        id: &str,
        layer: &str,
        mut feature: Feature,
    ) -> Result<Feature> {
        let layer = checked_layer(layer)?;
        checked(&feature.geometry)?;
        self.edit((project, kind, id), |annotations| {
            let Some(id) = feature.id.clone() else {
                annotations.last_id += 1;
                feature.id = Some(annotations.last_id.to_string());
                let features = &mut annotations.layers.entry(layer).or_default().features;
                features.push(feature.clone());
                return Ok(feature);
            };
            let kept = annotations.layers.get_mut(&layer).and_then(|l| {
                l.features
                    .iter_mut()
                    .find(|f| f.id.as_deref() == Some(id.as_str()))
            });
            let kept = kept.ok_or_else(|| StoreError::NotFound {
                kind: "annotation",
                id: format!("{id} in layer {layer:?}"),
            })?;
            *kept = feature.clone();
            Ok(feature)
        })
    }

    /// Puts `features` in place of layer `layer` of `kind` record `id`, each
    /// with a new ID, at once: as a job keeps thousands of outlines (P46).
    /// How many.
    pub fn replace_layer(
        &self,
        project: ProjectId,
        kind: RecordKind,
        id: &str,
        layer: &str,
        features: Vec<Feature>,
    ) -> Result<usize> {
        let layer = checked_layer(layer)?;
        for feature in &features {
            checked(&feature.geometry)?;
        }
        self.edit((project, kind, id), |annotations| {
            let mut kept = Layer::default();
            for mut feature in features {
                annotations.last_id += 1;
                feature.id = Some(annotations.last_id.to_string());
                kept.features.push(feature);
            }
            let count = kept.features.len();
            annotations.layers.insert(layer, kept);
            Ok(count)
        })
    }

    /// Removes feature `feature` from layer `layer` of `kind` record `id`,
    /// or with none, the whole layer.
    pub fn remove_annotation(
        &self,
        project: ProjectId,
        kind: RecordKind,
        id: &str,
        layer: &str,
        feature: Option<&str>,
    ) -> Result<()> {
        self.edit((project, kind, id), |annotations| {
            let missing = |id: String| StoreError::NotFound {
                kind: "annotation",
                id,
            };
            let Some(feature) = feature else {
                let removed = annotations.layers.remove(layer);
                return removed
                    .map(drop)
                    .ok_or_else(|| missing(format!("layer {layer:?}")));
            };
            let features = annotations.layers.get_mut(layer).map(|l| &mut l.features);
            let features = features.ok_or_else(|| missing(format!("layer {layer:?}")))?;
            let before = features.len();
            features.retain(|f| f.id.as_deref() != Some(feature));
            match features.len() < before {
                true => Ok(()),
                false => Err(missing(format!("{feature} in layer {layer:?}"))),
            }
        })
    }
}
