//! Parameters checked against their node's JSON Schema.
//!
//! OScope's own validator, for the part of JSON Schema 2020-12 that node
//! parameters use, and that `schemars` generates (*JSON Schema
//! Validation*, json-schema.org):
//!
//! - **any value:** `type`, `enum`, `const`;
//! - **numbers:** `minimum`, `maximum`, `exclusiveMinimum`,
//!   `exclusiveMaximum`, `multipleOf`;
//! - **strings:** `minLength`, `maxLength`;
//! - **arrays:** `items`, `prefixItems`, `minItems`, `maxItems`,
//!   `uniqueItems`;
//! - **objects:** `properties`, `required`, `additionalProperties`,
//!   `minProperties`, `maxProperties`;
//! - **combining:** `allOf`, `anyOf`, `oneOf`, `not`, and `$ref` to the
//!   schema's own `$defs`.
//!
//! Annotations, such as `title`, `description`, `default` and `format`, are
//! ignored. A schema with any other keyword, such as `pattern`, is refused
//! when its node is registered ([`check`]), so that none is ignored without
//! a word.

use serde_json::{Map, Value};

/// The keywords [`validate`] applies.
const ASSERTIONS: &[&str] = &[
    "type",
    "enum",
    "const",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "items",
    "prefixItems",
    "minItems",
    "maxItems",
    "uniqueItems",
    "properties",
    "required",
    "additionalProperties",
    "minProperties",
    "maxProperties",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "$ref",
];

/// The keywords it ignores.
const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$defs",
    "definitions",
    "$comment",
    "title",
    "description",
    "default",
    "examples",
    "format",
    "deprecated",
    "readOnly",
    "writeOnly",
];

/// How deep schemas may nest through `$ref`s: deeper is taken as a loop.
const DEPTH: usize = 64;

/// Checks that `schema` uses only what [`validate`] knows, and that its
/// `$ref`s resolve.
pub fn check(schema: &Value) -> Result<(), String> {
    check_at(schema, schema, "#")
}

fn check_at(root: &Value, schema: &Value, at: &str) -> Result<(), String> {
    let map = match schema {
        Value::Bool(_) => return Ok(()),
        Value::Object(map) => map,
        _ => return Err(format!("{at} isn't a schema")),
    };
    for (key, sub) in map {
        let here = format!("{at}/{key}");
        match key.as_str() {
            "properties" | "$defs" | "definitions" => {
                let named = sub.as_object().ok_or(format!("{here} isn't an object"))?;
                for (name, schema) in named {
                    check_at(root, schema, &format!("{here}/{name}"))?;
                }
            }
            "items" | "additionalProperties" | "not" => check_at(root, sub, &here)?,
            "prefixItems" | "allOf" | "anyOf" | "oneOf" => {
                let listed = sub.as_array().ok_or(format!("{here} isn't an array"))?;
                for (i, schema) in listed.iter().enumerate() {
                    check_at(root, schema, &format!("{here}/{i}"))?;
                }
            }
            "$ref" => {
                let target = sub.as_str().unwrap_or_default();
                if resolve(root, target).is_none() {
                    return Err(format!("{here}: `{target}` isn't in the schema"));
                }
            }
            known if ASSERTIONS.contains(&known) || ANNOTATIONS.contains(&known) => {}
            unknown => return Err(format!("{here}: `{unknown}` isn't supported")),
        }
    }
    Ok(())
}

/// The schema a `$ref` points to within `root`: `#/$defs/Level`.
fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    root.pointer(reference.strip_prefix('#')?)
}

/// Checks `value` against `schema`: each problem found, as a JSON pointer
/// into `value` and what is wrong there.
pub fn validate(schema: &Value, value: &Value) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    Validator {
        root: schema,
        problems: &mut problems,
    }
    .at(schema, value, "", 0);
    problems
}

struct Validator<'a> {
    root: &'a Value,
    problems: &'a mut Vec<(String, String)>,
}

impl Validator<'_> {
    fn fail(&mut self, at: &str, message: String) {
        let at = if at.is_empty() { "/" } else { at };
        self.problems.push((at.to_owned(), message));
    }

    /// Whether `value` passes `schema`, without saying why not.
    fn passes(&self, schema: &Value, value: &Value, depth: usize) -> bool {
        let mut problems = Vec::new();
        Validator {
            root: self.root,
            problems: &mut problems,
        }
        .at(schema, value, "", depth);
        problems.is_empty()
    }

    fn at(&mut self, schema: &Value, value: &Value, at: &str, depth: usize) {
        let map = match schema {
            Value::Bool(false) => return self.fail(at, "nothing is allowed here".into()),
            Value::Object(map) => map,
            _ => return,
        };
        if depth > DEPTH {
            return self.fail(at, "the schema nests too deep".into());
        }
        let reference = map.get("$ref").and_then(Value::as_str);
        if let Some(target) = reference.and_then(|r| resolve(self.root, r)) {
            self.at(target, value, at, depth + 1);
        }
        if let Some(types) = map.get("type") {
            let names: Vec<&str> = match types {
                Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
                name => name.as_str().into_iter().collect(),
            };
            if !names.iter().any(|name| is_type(value, name)) {
                let needed = names.join(" or ");
                return self.fail(at, format!("{} where {needed} is needed", describe(value)));
            }
        }
        if let Some(Value::Array(choices)) = map.get("enum")
            && !choices.iter().any(|choice| same(choice, value))
        {
            self.fail(
                at,
                format!("{value} isn't one of {}", Value::Array(choices.clone())),
            );
        }
        if let Some(only) = map.get("const")
            && !same(only, value)
        {
            self.fail(at, format!("{value} isn't {only}"));
        }
        match value {
            Value::Number(number) => self.number(map, number.as_f64().unwrap_or(f64::NAN), at),
            Value::String(text) => self.string(map, text, at),
            Value::Array(items) => self.array(map, items, at, depth),
            Value::Object(fields) => self.object(map, fields, at, depth),
            _ => {}
        }
        let listed = |key| map.get(key).and_then(Value::as_array);
        for schema in listed("allOf").into_iter().flatten() {
            self.at(schema, value, at, depth + 1);
        }
        if let Some(any) = listed("anyOf")
            && !any
                .iter()
                .any(|schema| self.passes(schema, value, depth + 1))
        {
            self.fail(at, "matches none of the choices".into());
        }
        if let Some(one) = listed("oneOf") {
            let passed = one
                .iter()
                .filter(|s| self.passes(s, value, depth + 1))
                .count();
            if passed != 1 {
                self.fail(
                    at,
                    format!("matches {passed} of the choices, where one is needed"),
                );
            }
        }
        if let Some(not) = map.get("not")
            && self.passes(not, value, depth + 1)
        {
            self.fail(at, "matches what it mustn't".into());
        }
    }

    fn number(&mut self, map: &Map<String, Value>, x: f64, at: &str) {
        let bound = |key| map.get(key).and_then(Value::as_f64);
        if let Some(min) = bound("minimum")
            && x < min
        {
            self.fail(at, format!("{x} is below the minimum, {min}"));
        }
        if let Some(max) = bound("maximum")
            && x > max
        {
            self.fail(at, format!("{x} is above the maximum, {max}"));
        }
        if let Some(min) = bound("exclusiveMinimum")
            && x <= min
        {
            self.fail(at, format!("{x} isn't above {min}"));
        }
        if let Some(max) = bound("exclusiveMaximum")
            && x >= max
        {
            self.fail(at, format!("{x} isn't below {max}"));
        }
        if let Some(step) = bound("multipleOf")
            && step > 0.0
        {
            let times = x / step;
            if (times - times.round()).abs() > 1e-9 * times.abs().max(1.0) {
                self.fail(at, format!("{x} isn't a multiple of {step}"));
            }
        }
    }

    fn string(&mut self, map: &Map<String, Value>, text: &str, at: &str) {
        let length = text.chars().count() as u64;
        let bound = |key| map.get(key).and_then(Value::as_u64);
        if let Some(min) = bound("minLength")
            && length < min
        {
            self.fail(at, format!("is shorter than {min} characters"));
        }
        if let Some(max) = bound("maxLength")
            && length > max
        {
            self.fail(at, format!("is longer than {max} characters"));
        }
    }

    fn array(&mut self, map: &Map<String, Value>, items: &[Value], at: &str, depth: usize) {
        let prefix = map.get("prefixItems").and_then(Value::as_array);
        for (i, item) in items.iter().enumerate() {
            let schema = prefix.and_then(|p| p.get(i)).or_else(|| map.get("items"));
            if let Some(schema) = schema {
                self.at(schema, item, &format!("{at}/{i}"), depth + 1);
            }
        }
        let count = items.len() as u64;
        let bound = |key| map.get(key).and_then(Value::as_u64);
        if let Some(min) = bound("minItems")
            && count < min
        {
            self.fail(at, format!("has fewer than {min} items"));
        }
        if let Some(max) = bound("maxItems")
            && count > max
        {
            self.fail(at, format!("has more than {max} items"));
        }
        let unique = map.get("uniqueItems") == Some(&Value::Bool(true));
        if unique && (1..items.len()).any(|i| items[..i].iter().any(|a| same(a, &items[i]))) {
            self.fail(at, "has an item twice".into());
        }
    }

    fn object(
        &mut self,
        map: &Map<String, Value>,
        fields: &Map<String, Value>,
        at: &str,
        depth: usize,
    ) {
        let properties = map.get("properties").and_then(Value::as_object);
        for (name, field) in fields {
            let here = format!("{at}/{}", escape(name));
            match (
                properties.and_then(|p| p.get(name)),
                map.get("additionalProperties"),
            ) {
                (Some(schema), _) | (None, Some(schema)) => {
                    self.at(schema, field, &here, depth + 1)
                }
                (None, None) => {}
            }
        }
        let required = map.get("required").and_then(Value::as_array);
        for name in required.into_iter().flatten().filter_map(Value::as_str) {
            if !fields.contains_key(name) {
                self.fail(&format!("{at}/{}", escape(name)), "is missing".into());
            }
        }
        let count = fields.len() as u64;
        let bound = |key| map.get(key).and_then(Value::as_u64);
        if let Some(min) = bound("minProperties")
            && count < min
        {
            self.fail(at, format!("has fewer than {min} members"));
        }
        if let Some(max) = bound("maxProperties")
            && count > max
        {
            self.fail(at, format!("has more than {max} members"));
        }
    }
}

fn is_type(value: &Value, name: &str) -> bool {
    match name {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|x| x.fract() == 0.0),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

/// Equality as JSON Schema has it: numbers by value, so 1 and 1.0 are the
/// same.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, a)| y.get(key).is_some_and(|b| same(a, b)))
        }
        _ => a == b,
    }
}

fn describe(value: &Value) -> String {
    match value {
        Value::String(_) => "a string".into(),
        Value::Array(_) => "an array".into(),
        Value::Object(_) => "an object".into(),
        scalar => scalar.to_string(),
    }
}

/// A member's name in a JSON pointer (RFC 6901).
fn escape(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The problems, sorted: JSON objects may keep their members' order.
    fn problems(schema: Value, value: Value) -> Vec<String> {
        assert_eq!(check(&schema), Ok(()));
        let mut found: Vec<String> = validate(&schema, &value)
            .into_iter()
            .map(|(at, message)| format!("{at}: {message}"))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn values_are_checked_by_type_range_and_shape() {
        let schema = json!({
            "type": "object",
            "properties": {
                "radius": { "type": "integer", "minimum": 0, "maximum": 64 },
                "level": { "type": ["number", "null"], "exclusiveMinimum": 0 },
                "mode": { "enum": ["fast", "exact"] },
                "name": { "type": "string", "maxLength": 3 },
                "gains": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }
            },
            "required": ["radius"],
            "additionalProperties": false
        });
        let fine = json!({ "radius": 3.0, "level": null, "mode": "fast", "name": "ab", "gains": [1, 2, 3] });
        assert_eq!(problems(schema.clone(), fine), Vec::<String>::new());
        let bad = json!({ "radius": 2.5, "level": 0, "mode": "slow", "name": "abcd", "gains": [1, "x"], "extra": 1 });
        assert_eq!(
            problems(schema.clone(), bad),
            [
                "/extra: nothing is allowed here",
                "/gains/1: a string where number is needed",
                "/gains: has fewer than 3 items",
                "/level: 0 isn't above 0",
                "/mode: \"slow\" isn't one of [\"fast\",\"exact\"]",
                "/name: is longer than 3 characters",
                "/radius: 2.5 where integer is needed",
            ]
        );
        assert_eq!(problems(schema.clone(), json!({})), ["/radius: is missing"]);
        assert_eq!(
            problems(schema, json!([])),
            ["/: an array where object is needed"]
        );
    }

    #[test]
    fn references_and_choices_are_followed() {
        let schema = json!({
            "$defs": { "Level": { "type": "number", "minimum": 0, "maximum": 1 } },
            "properties": {
                "low": { "$ref": "#/$defs/Level" },
                "either": { "oneOf": [{ "type": "integer" }, { "type": "number", "minimum": 10 }] },
                "any": { "anyOf": [{ "const": 1 }, { "type": "string" }] },
                "not": { "not": { "type": "string" } }
            }
        });
        let fine = json!({ "low": 0.5, "either": 3, "any": "x", "not": 1 });
        assert_eq!(problems(schema.clone(), fine), Vec::<String>::new());
        let bad = json!({ "low": 2, "either": 12, "any": 2, "not": "x" });
        assert_eq!(
            problems(schema, bad),
            [
                "/any: matches none of the choices",
                "/either: matches 2 of the choices, where one is needed",
                "/low: 2 is above the maximum, 1",
                "/not: matches what it mustn't",
            ]
        );
    }

    #[test]
    fn schemas_with_what_it_doesn_t_know_are_refused() {
        assert_eq!(
            check(&json!({ "properties": { "name": { "type": "string", "pattern": "^a" } } })),
            Err("#/properties/name/pattern: `pattern` isn't supported".into())
        );
        assert_eq!(
            check(&json!({ "$ref": "#/$defs/Missing" })),
            Err("#/$ref: `#/$defs/Missing` isn't in the schema".into())
        );
        // A loop through references ends, as a problem.
        let looped = json!({ "$defs": { "A": { "$ref": "#/$defs/A" } }, "$ref": "#/$defs/A" });
        assert_eq!(check(&looped), Ok(()));
        assert_eq!(
            validate(&looped, &json!(1)),
            [("/".to_owned(), "the schema nests too deep".to_owned())]
        );
    }
}
