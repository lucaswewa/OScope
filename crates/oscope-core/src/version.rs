//! Versioned documents: JSON that OScope writes and reads back later, such as
//! recipes, configurations and project files.
//!
//! A document carries its schema's name and version beside its own fields:
//! `{"schema": "oscope.recipe", "version": 2, …}`. Reading one written by an
//! older OScope upgrades it a version at a time, through
//! [`Versioned::upgrade`]. Reading one written by a newer OScope fails, rather
//! than dropping what this build doesn't understand.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::error::{Error, Result};

/// The field that names a document's schema.
pub const SCHEMA_FIELD: &str = "schema";
/// The field that gives a document's version.
pub const VERSION_FIELD: &str = "version";

/// A type saved as a versioned document. Its fields must serialise as a JSON
/// object, without [`SCHEMA_FIELD`] or [`VERSION_FIELD`] among them.
pub trait Versioned: Serialize + DeserializeOwned {
    /// The schema's name, such as `oscope.recipe`.
    const SCHEMA: &'static str;
    /// The version this build writes, counted from 1.
    const VERSION: u32;

    /// Turns the fields of a version `from` document into those of version
    /// `from + 1`. By default no older version can be upgraded.
    fn upgrade(from: u32, fields: Map<String, Value>) -> Result<Map<String, Value>> {
        let _ = fields;
        Err(Error::CantUpgrade {
            schema: Self::SCHEMA,
            found: from,
        })
    }
}

fn not_a<T: Versioned>(reason: impl Into<String>) -> Error {
    Error::NotADocument {
        schema: T::SCHEMA,
        reason: reason.into(),
    }
}

fn mismatch<T: Versioned>(source: serde_json::Error) -> Error {
    Error::Mismatch {
        schema: T::SCHEMA,
        source,
    }
}

/// `value` as a document: its schema, its version, and its fields.
pub fn to_document<T: Versioned>(value: &T) -> Result<Value> {
    let Value::Object(fields) = serde_json::to_value(value).map_err(mismatch::<T>)? else {
        return Err(not_a::<T>("its fields aren't a JSON object"));
    };
    if let Some(field) = [SCHEMA_FIELD, VERSION_FIELD]
        .into_iter()
        .find(|f| fields.contains_key(*f))
    {
        return Err(not_a::<T>(format!(
            "it has a field of its own named {field:?}"
        )));
    }
    let mut document = Map::new();
    document.insert(SCHEMA_FIELD.into(), T::SCHEMA.into());
    document.insert(VERSION_FIELD.into(), T::VERSION.into());
    document.extend(fields);
    Ok(Value::Object(document))
}

/// The value a document holds, upgraded first if an older OScope wrote it.
pub fn from_document<T: Versioned>(document: Value) -> Result<T> {
    let Value::Object(mut fields) = document else {
        return Err(not_a::<T>("it isn't a JSON object"));
    };
    match fields.remove(SCHEMA_FIELD) {
        Some(Value::String(schema)) if schema == T::SCHEMA => {}
        Some(other) => return Err(not_a::<T>(format!("its schema is {other}"))),
        None => return Err(not_a::<T>("it names no schema")),
    }
    let version = fields
        .remove(VERSION_FIELD)
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v >= 1)
        .ok_or_else(|| not_a::<T>("it has no version, a whole number from 1"))?;
    if version > T::VERSION {
        return Err(Error::NewerVersion {
            schema: T::SCHEMA,
            found: version,
            current: T::VERSION,
        });
    }
    for from in version..T::VERSION {
        fields = T::upgrade(from, fields)?;
    }
    serde_json::from_value(Value::Object(fields)).map_err(mismatch::<T>)
}
