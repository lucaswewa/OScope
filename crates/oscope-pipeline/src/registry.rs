//! The nodes a server has.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::Arc;

use crate::node::{Descriptor, Node, Port};
use crate::schema;

/// The nodes a server has, by ID (ADR-0017). It is filled at start-up, when
/// `teta-wot` fixes its Things too.
#[derive(Clone, Default)]
pub struct Registry {
    nodes: BTreeMap<String, Arc<dyn Node>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Adds `node`. Refused when a node has its ID already, or when its
    /// descriptor is malformed: an ID or port names that aren't names, a
    /// port named twice, no outputs, neither the CPU nor CUDA, tileable with
    /// outputs that aren't rasters, or parameters whose schema
    /// [`schema::check`] refuses.
    pub fn register(&mut self, node: Arc<dyn Node>) -> Result<(), String> {
        let descriptor = node.descriptor();
        let id = &descriptor.id;
        check(descriptor).map_err(|problem| format!("node `{id}`: {problem}"))?;
        if self.nodes.contains_key(id) {
            return Err(format!("node `{id}` is registered already"));
        }
        self.nodes.insert(id.clone(), node);
        Ok(())
    }

    /// The node with ID `id`.
    pub fn get(&self, id: &str) -> Option<&Arc<dyn Node>> {
        self.nodes.get(id)
    }

    /// Every node's descriptor, by ID.
    pub fn descriptors(&self) -> impl Iterator<Item = &Descriptor> {
        self.nodes.values().map(|node| node.descriptor())
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.nodes.keys()).finish()
    }
}

/// Whether `text` is a name: letters, digits and `_`, not starting with a
/// digit.
pub(crate) fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check(descriptor: &Descriptor) -> Result<(), String> {
    if !descriptor.id.split('.').all(is_name) {
        return Err("its ID isn't dotted names".into());
    }
    let ports = |ports: &[Port], side: &str| {
        let mut seen = HashSet::new();
        for port in ports {
            if !is_name(&port.name) || !seen.insert(&port.name) {
                return Err(format!(
                    "{side} `{}` isn't a name, or is there twice",
                    port.name
                ));
            }
        }
        Ok(())
    };
    ports(&descriptor.inputs, "input")?;
    ports(&descriptor.outputs, "output")?;
    let can = descriptor.capabilities;
    if descriptor.outputs.is_empty() {
        return Err("it has no outputs".into());
    }
    if !can.cpu && !can.cuda {
        return Err("it runs on neither the CPU nor the GPU".into());
    }
    if can.tileable && !descriptor.outputs.iter().all(|port| port.kind.is_raster()) {
        return Err("it is tileable, but gives what isn't a raster".into());
    }
    schema::check(&descriptor.params)
        .map_err(|problem| format!("its parameters' schema: {problem}"))
}
