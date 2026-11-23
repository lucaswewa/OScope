//! Recipes: nodes, and the edges between their ports, as versioned JSON.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::node::Port;

/// The recipe format this OScope reads and writes.
pub const FORMAT: u32 = 1;

/// The name a recipe's own inputs go by in its edges: `input.image`.
pub const INPUT: &str = "input";

/// A recipe (ADR-0017): nodes with their parameters, the edges between
/// their ports, and the recipe's own inputs and outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Its format: 1, for now.
    pub format: u32,
    /// A name for people.
    #[serde(default)]
    pub name: String,
    /// What it takes, each named and typed: `input.<name>` in its edges.
    #[serde(default)]
    pub inputs: Vec<Port>,
    /// Its nodes.
    pub nodes: Vec<NodeUse>,
    /// Its edges.
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// What it gives.
    pub outputs: Vec<RecipeOutput>,
}

/// A node in a recipe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeUse {
    /// Its ID in the recipe: letters, digits and `_`, and not `input`.
    pub id: String,
    /// Which node it is: its descriptor's ID.
    pub node: String,
    /// The node's version the recipe was made for: the registry's must be
    /// the same, when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Its parameters, which the node's schema checks.
    #[serde(default = "no_params")]
    pub params: serde_json::Value,
}

fn no_params() -> serde_json::Value {
    serde_json::json!({})
}

/// An edge: from a node's output, or a recipe input, to a node's input, each
/// `node.port`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    /// The output: `blur.image`, or `input.image` for a recipe input.
    pub from: String,
    /// The input: `threshold.image`.
    pub to: String,
}

/// One of a recipe's outputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeOutput {
    /// Its name.
    pub name: String,
    /// The node's output, or recipe input, it gives: `node.port`.
    pub from: String,
}

impl Recipe {
    /// A recipe from its JSON.
    pub fn from_json(text: &str) -> Result<Recipe, RecipeError> {
        serde_json::from_str(text).map_err(|error| RecipeError::one("recipe", error.to_string()))
    }

    /// Its JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Sets parameter `name` of node `id` to `value`, as an editor would.
    pub fn set_param(
        &mut self,
        id: &str,
        name: &str,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let node = self.nodes.iter_mut().find(|node| node.id == id);
        let node = node.ok_or_else(|| format!("the recipe has no node `{id}`"))?;
        let params = node.params.as_object_mut();
        let params = params.ok_or_else(|| format!("node `{id}`'s parameters aren't an object"))?;
        params.insert(name.to_owned(), value);
        Ok(())
    }
}

/// What is wrong with a recipe, and where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Problem {
    /// Where: `nodes[1] (blur)`, `edges[0]`, `outputs[2]`, with a JSON
    /// pointer into a node's parameters, or `recipe` for all of it.
    pub at: String,
    /// The node it is at, by its ID in the recipe, if it is at one: `input`
    /// for the recipe's own inputs (W7.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The node's port it is at, if it is at one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// What.
    pub message: String,
}

impl Problem {
    /// A problem at `at`, at no node.
    pub fn new(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            node: None,
            port: None,
            message: message.into(),
        }
    }

    /// The same, at `node`, and `port` if given.
    #[must_use]
    pub fn on(mut self, node: &str, port: Option<&str>) -> Self {
        self.node = Some(node.to_owned());
        self.port = port.map(str::to_owned);
        self
    }

    /// The same, at the node and port `reference`, `node.port`, names.
    #[must_use]
    pub fn on_port(self, reference: &str) -> Self {
        match reference.split_once('.') {
            Some((node, port)) => self.on(node, Some(port)),
            None => self,
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.message)
    }
}

/// A recipe that can't run: every problem found in it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub struct RecipeError {
    /// The problems.
    pub problems: Vec<Problem>,
}

impl RecipeError {
    pub(crate) fn one(at: &str, message: String) -> RecipeError {
        RecipeError {
            problems: vec![Problem::new(at, message)],
        }
    }
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let problems: Vec<String> = self.problems.iter().map(ToString::to_string).collect();
        write!(f, "{}", problems.join("; "))
    }
}
