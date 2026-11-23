//! A recipe checked against the registry, its steps ordered and placed:
//! what the engine runs.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use crate::memo::hash_json;
use crate::node::{Device, Node, Port, Step};
use crate::recipe::{FORMAT, INPUT, NodeUse, Problem, Recipe, RecipeError};
use crate::registry::{Registry, is_name};
use crate::schema;
use crate::value::PortType;

/// Where a step's input comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// The recipe's input with this index.
    Input(usize),
    /// A step's output: the step's index, and the output's.
    Step(usize, usize),
}

impl Source {
    fn step(self) -> Option<usize> {
        match self {
            Source::Step(step, _) => Some(step),
            Source::Input(_) => None,
        }
    }
}

/// A step of a plan.
pub(crate) struct Planned {
    pub id: String,
    pub node: Arc<dyn Node>,
    pub step: Arc<dyn Step>,
    pub params: serde_json::Value,
    pub device: Device,
    pub halo: u32,
    /// Where each of the node's inputs comes from.
    pub sources: Vec<Option<Source>>,
    /// The steps it reads, each once.
    pub producers: Vec<usize>,
    /// The steps reading it, each once.
    pub readers: Vec<usize>,
    /// Whether a recipe output reads it, so its values are kept to the end.
    pub output: bool,
    /// Its own part of its memo key: the node, its version, its parameters
    /// and its device.
    pub key: u64,
}

/// A recipe checked against a registry (ADR-0017): its steps in an order
/// that runs each after those it reads, each placed on the CPU or the GPU.
pub struct Plan {
    pub(crate) name: String,
    pub(crate) inputs: Vec<Port>,
    pub(crate) steps: Vec<Planned>,
    pub(crate) outputs: Vec<(String, Source)>,
    halo: u32,
    transfers: usize,
}

/// A node of the recipe, as checking finds it.
struct Found {
    node: Arc<dyn Node>,
    step: Option<Arc<dyn Step>>,
    sources: Vec<Option<Source>>,
}

fn problem(at: impl Into<String>, message: impl Into<String>) -> Problem {
    let (at, message) = (at.into(), message.into());
    Problem { at, message }
}

impl Plan {
    /// Checks `recipe` against `registry` and plans it, with the GPU for
    /// nodes that have CUDA steps when `gpu` says there is one. Every
    /// problem found is given:
    ///
    /// - the format, and names;
    /// - nodes not in the registry, other versions, and parameters their
    ///   schemas refuse;
    /// - edges from or to ports that aren't there, or between ports of
    ///   different types; inputs with two edges, and inputs without one;
    /// - cycles;
    /// - nodes that run only on a GPU, where there is none.
    pub fn new(registry: &Registry, recipe: &Recipe, gpu: bool) -> Result<Plan, RecipeError> {
        let mut problems = Vec::new();
        if recipe.format != FORMAT {
            let message = format!(
                "format {} isn't the one this OScope reads, {FORMAT}",
                recipe.format
            );
            problems.push(problem("format", message));
        }
        let mut inputs = HashMap::new();
        for (i, port) in recipe.inputs.iter().enumerate() {
            if !is_name(&port.name) || inputs.insert(port.name.as_str(), i).is_some() {
                problems.push(problem(
                    format!("inputs[{i}]"),
                    "it isn't a name, or names another input too",
                ));
            }
        }
        let mut ids = HashMap::new();
        let mut found = Vec::new();
        for (i, used) in recipe.nodes.iter().enumerate() {
            let at = format!("nodes[{i}] ({})", used.id);
            if !is_name(&used.id) || used.id == INPUT || ids.insert(used.id.as_str(), i).is_some() {
                problems.push(problem(
                    &at,
                    "its ID isn't a name, is `input`, or is another node's too",
                ));
            }
            found.push(find(registry, used, gpu, &at, &mut problems));
        }
        let lookup = Lookup {
            recipe,
            inputs: &inputs,
            ids: &ids,
        };
        let mut fed = HashSet::new();
        for (e, edge) in recipe.edges.iter().enumerate() {
            let at = format!("edges[{e}]");
            let (from, to) = (lookup.from(&found, &edge.from), lookup.to(&found, &edge.to));
            if let Ok(Some((node, port, _))) = to {
                fed.insert((node, port));
            }
            match (from, to) {
                (Ok(Some((source, gives))), Ok(Some((node, port, takes)))) => {
                    let slot = found[node].as_mut().map(|f| &mut f.sources[port]);
                    if gives != takes {
                        let message = format!(
                            "`{}` gives {gives}, and `{}` takes {takes}",
                            edge.from, edge.to
                        );
                        problems.push(problem(&at, message));
                    } else if let Some(slot) = slot
                        && slot.replace(source).is_some()
                    {
                        problems.push(problem(&at, format!("`{}` has another edge too", edge.to)));
                    }
                }
                (from, to) => {
                    for message in [from.err(), to.err()].into_iter().flatten() {
                        problems.push(problem(&at, message));
                    }
                }
            }
        }
        for (n, found) in found.iter().enumerate() {
            let Some(found) = found else { continue };
            for (p, port) in found.node.descriptor().inputs.iter().enumerate() {
                if !port.optional && !fed.contains(&(n, p)) {
                    let at = format!("nodes[{n}] ({})", recipe.nodes[n].id);
                    problems.push(problem(
                        at,
                        format!("its input `{}` has no edge", port.name),
                    ));
                }
            }
        }
        let mut names = HashSet::new();
        let mut outputs = Vec::new();
        for (o, output) in recipe.outputs.iter().enumerate() {
            let at = format!("outputs[{o}]");
            if !is_name(&output.name) || !names.insert(output.name.as_str()) {
                problems.push(problem(
                    &at,
                    "its name isn't a name, or is another output's too",
                ));
            }
            match lookup.from(&found, &output.from) {
                Ok(Some((source, _))) => outputs.push((output.name.clone(), source)),
                Ok(None) => {}
                Err(message) => problems.push(problem(&at, message)),
            }
        }
        if recipe.outputs.is_empty() {
            problems.push(problem("outputs", "the recipe gives nothing"));
        }
        let order = order(&found).unwrap_or_else(|cycle| {
            let names: Vec<&str> = cycle.iter().map(|&n| recipe.nodes[n].id.as_str()).collect();
            problems.push(problem(
                "edges",
                format!("they make a cycle: {}", names.join(" → ")),
            ));
            Vec::new()
        });
        if !problems.is_empty() {
            return Err(RecipeError { problems });
        }
        Ok(plan(recipe, found, order, outputs, gpu))
    }

    /// The recipe's inputs.
    pub fn inputs(&self) -> &[Port] {
        &self.inputs
    }

    /// The recipe's outputs: their names and types.
    pub fn outputs(&self) -> Vec<(&str, PortType)> {
        let kind = |source: Source| match source {
            Source::Input(i) => self.inputs[i].kind,
            Source::Step(s, p) => self.steps[s].node.descriptor().outputs[p].kind,
        };
        self.outputs
            .iter()
            .map(|(name, source)| (name.as_str(), kind(*source)))
            .collect()
    }

    /// The steps' IDs, in the order they start in when one runs at a time.
    pub fn order(&self) -> Vec<&str> {
        self.steps.iter().map(|step| step.id.as_str()).collect()
    }

    /// Where step `id` runs.
    pub fn device(&self, id: &str) -> Option<Device> {
        self.steps
            .iter()
            .find(|step| step.id == id)
            .map(|step| step.device)
    }

    /// The pixels around a tile that a tiled run reads: the largest sum of
    /// halos along a path from an input to an output.
    pub fn halo(&self) -> u32 {
        self.halo
    }

    /// The copies of images between memory and the GPU it plans: each image
    /// read on the other side, once for that side.
    pub fn transfers(&self) -> usize {
        self.transfers
    }

    /// The steps that can't run live.
    pub fn not_live(&self) -> Vec<&str> {
        self.lacking(|step| step.node.descriptor().capabilities.live)
    }

    /// The steps that can't run tile by tile.
    pub fn not_tileable(&self) -> Vec<&str> {
        self.lacking(|step| step.node.descriptor().capabilities.tileable)
    }

    fn lacking(&self, can: impl Fn(&Planned) -> bool) -> Vec<&str> {
        let steps = self.steps.iter().filter(|step| !can(step));
        steps.map(|step| step.id.as_str()).collect()
    }

    pub(crate) fn uses_gpu(&self) -> bool {
        self.steps.iter().any(|step| step.device == Device::Cuda)
    }

    fn name_of(&self, source: Source) -> String {
        match source {
            Source::Input(i) => format!("{INPUT}.{}", self.inputs[i].name),
            Source::Step(s, p) => {
                let step = &self.steps[s];
                format!("{}.{}", step.id, step.node.descriptor().outputs[p].name)
            }
        }
    }
}

impl fmt::Debug for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plan")
            .field("name", &self.name)
            .field("order", &self.order())
            .field("halo", &self.halo)
            .field("transfers", &self.transfers)
            .finish()
    }
}

impl fmt::Display for Plan {
    /// The graph: each step, in order, with its node, its device, what it
    /// reads and its parameters; then what the recipe gives.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = if self.name.is_empty() {
            "a recipe"
        } else {
            &self.name
        };
        writeln!(
            f,
            "{name}: {} steps; tiles read {} px around them; {} copies between memory and the GPU",
            self.steps.len(),
            self.halo,
            self.transfers
        )?;
        for step in &self.steps {
            let descriptor = step.node.descriptor();
            let node = format!("{} v{}", descriptor.id, descriptor.version);
            let device = serde_json::to_value(step.device).unwrap_or_default();
            let reads = descriptor.inputs.iter().zip(&step.sources);
            let reads: Vec<String> = reads
                .filter_map(|(port, source)| {
                    Some(format!("{} ← {}", port.name, self.name_of((*source)?)))
                })
                .collect();
            let device = device.as_str().unwrap_or("?");
            writeln!(
                f,
                "  {:<10} {node:<20} {device:<4}  {}  {}",
                step.id,
                reads.join(", "),
                step.params
            )?;
        }
        let gives: Vec<String> = self
            .outputs
            .iter()
            .map(|(name, s)| format!("{name} ← {}", self.name_of(*s)))
            .collect();
        write!(f, "  gives {}", gives.join(", "))
    }
}

/// Finds a recipe's node in the registry, and checks its version, devices
/// and parameters.
fn find(
    registry: &Registry,
    used: &NodeUse,
    gpu: bool,
    at: &str,
    problems: &mut Vec<Problem>,
) -> Option<Found> {
    let Some(node) = registry.get(&used.node) else {
        problems.push(problem(at, format!("there is no node `{}`", used.node)));
        return None;
    };
    let descriptor = node.descriptor();
    if let Some(version) = used.version
        && version != descriptor.version
    {
        let message = format!(
            "it is for version {version} of `{}`, and this is {}",
            used.node, descriptor.version
        );
        problems.push(problem(at, message));
    }
    let can = descriptor.capabilities;
    if !can.cpu && !(gpu && can.cuda) {
        problems.push(problem(
            at,
            format!("`{}` runs only on a GPU, and there is none", used.node),
        ));
    }
    let refused = schema::validate(&descriptor.params, &used.params);
    for (pointer, message) in &refused {
        problems.push(problem(format!("{at} params{pointer}"), message.clone()));
    }
    let step = if refused.is_empty() {
        let prepared = node.prepare(&used.params);
        let prepared = prepared
            .map_err(|error| problems.push(problem(at, format!("its parameters: {error}"))));
        prepared.ok().map(Arc::from)
    } else {
        None
    };
    let sources = vec![None; descriptor.inputs.len()];
    let node = Arc::clone(node);
    Some(Found {
        node,
        step,
        sources,
    })
}

/// Resolves `node.port` references.
struct Lookup<'a> {
    recipe: &'a Recipe,
    inputs: &'a HashMap<&'a str, usize>,
    ids: &'a HashMap<&'a str, usize>,
}

impl Lookup<'_> {
    fn split<'r>(&self, reference: &'r str) -> Result<(&'r str, &'r str), String> {
        reference
            .split_once('.')
            .ok_or(format!("`{reference}` isn't `node.port`"))
    }

    /// What an edge or output comes from, and its type: `None` for a node
    /// not in the registry, which is a problem already.
    fn from(
        &self,
        found: &[Option<Found>],
        reference: &str,
    ) -> Result<Option<(Source, PortType)>, String> {
        let (id, port) = self.split(reference)?;
        if id == INPUT {
            let &i = self
                .inputs
                .get(port)
                .ok_or(format!("the recipe has no input `{port}`"))?;
            return Ok(Some((Source::Input(i), self.recipe.inputs[i].kind)));
        }
        let &n = self.ids.get(id).ok_or(format!("there is no node `{id}`"))?;
        let Some(node) = &found[n] else {
            return Ok(None);
        };
        let outputs = &node.node.descriptor().outputs;
        let p = outputs.iter().position(|output| output.name == port);
        let p = p.ok_or(format!("`{id}` has no output `{port}`"))?;
        Ok(Some((Source::Step(n, p), outputs[p].kind)))
    }

    /// The node and input an edge goes to, and its type.
    fn to(
        &self,
        found: &[Option<Found>],
        reference: &str,
    ) -> Result<Option<(usize, usize, PortType)>, String> {
        let (id, port) = self.split(reference)?;
        let &n = self.ids.get(id).ok_or(format!("there is no node `{id}`"))?;
        let Some(node) = &found[n] else {
            return Ok(None);
        };
        let inputs = &node.node.descriptor().inputs;
        let p = inputs.iter().position(|input| input.name == port);
        let p = p.ok_or(format!("`{id}` has no input `{port}`"))?;
        Ok(Some((n, p, inputs[p].kind)))
    }
}

/// The nodes in an order that puts each after those it reads, and otherwise
/// in the recipe's order (Kahn's algorithm); or a cycle, if they make one.
fn order(found: &[Option<Found>]) -> Result<Vec<usize>, Vec<usize>> {
    let reads = |n: usize| -> Vec<usize> {
        let sources = found[n]
            .iter()
            .flat_map(|found| found.sources.iter().flatten());
        sources.filter_map(|source| source.step()).collect()
    };
    let count = found.len();
    let mut waiting: Vec<usize> = (0..count).map(|n| reads(n).len()).collect();
    let mut readers = vec![Vec::new(); count];
    for n in 0..count {
        for producer in reads(n) {
            readers[producer].push(n);
        }
    }
    let mut ready: BTreeSet<usize> = (0..count).filter(|&n| waiting[n] == 0).collect();
    let mut order = Vec::with_capacity(count);
    while let Some(n) = ready.pop_first() {
        order.push(n);
        for &reader in &readers[n] {
            waiting[reader] -= 1;
            if waiting[reader] == 0 {
                ready.insert(reader);
            }
        }
    }
    if order.len() == count {
        return Ok(order);
    }
    // Every node left reads one left: walk back through them until one
    // comes again.
    let mut path = vec![(0..count).find(|&n| waiting[n] > 0).unwrap_or(0)];
    loop {
        let last = path[path.len() - 1];
        let Some(back) = reads(last).into_iter().find(|&n| waiting[n] > 0) else {
            return Err(path);
        };
        if let Some(start) = path.iter().position(|&n| n == back) {
            let mut cycle = path.split_off(start);
            cycle.reverse();
            cycle.push(cycle[0]);
            return Err(cycle);
        }
        path.push(back);
    }
}

/// Plans a recipe that has passed its checks: every node found and
/// prepared, in `order`.
fn plan(
    recipe: &Recipe,
    mut found: Vec<Option<Found>>,
    order: Vec<usize>,
    outputs: Vec<(String, Source)>,
    gpu: bool,
) -> Plan {
    // Only the steps the outputs need.
    let mut needed = vec![false; found.len()];
    let mut stack: Vec<usize> = outputs
        .iter()
        .filter_map(|(_, source)| source.step())
        .collect();
    while let Some(n) = stack.pop() {
        if !std::mem::replace(&mut needed[n], true)
            && let Some(node) = &found[n]
        {
            stack.extend(
                node.sources
                    .iter()
                    .flatten()
                    .filter_map(|source| source.step()),
            );
        }
    }
    let order: Vec<usize> = order.into_iter().filter(|&n| needed[n]).collect();
    let mut index = vec![usize::MAX; found.len()];
    for (s, &n) in order.iter().enumerate() {
        index[n] = s;
    }
    let renumber = |source: Source| match source {
        Source::Step(n, port) => Source::Step(index[n], port),
        input => input,
    };
    let mut steps: Vec<Planned> = Vec::with_capacity(order.len());
    for &n in &order {
        let Some(Found {
            node,
            step: Some(step),
            sources,
        }) = found[n].take()
        else {
            unreachable!("a recipe that passed its checks has every node found and prepared");
        };
        let sources: Vec<Option<Source>> = sources.into_iter().map(|s| s.map(renumber)).collect();
        let mut producers: Vec<usize> = sources.iter().flatten().filter_map(|s| s.step()).collect();
        producers.sort_unstable();
        producers.dedup();
        let can = node.descriptor().capabilities;
        steps.push(Planned {
            id: recipe.nodes[n].id.clone(),
            halo: step.halo().unwrap_or(can.halo),
            device: if gpu && can.cuda {
                Device::Cuda
            } else {
                Device::Cpu
            },
            params: recipe.nodes[n].params.clone(),
            key: 0,
            sources,
            producers,
            readers: Vec::new(),
            output: false,
            node,
            step,
        });
    }
    let outputs: Vec<(String, Source)> = outputs
        .into_iter()
        .map(|(name, s)| (name, renumber(s)))
        .collect();
    for s in 0..steps.len() {
        for p in steps[s].producers.clone() {
            steps[p].readers.push(s);
        }
    }
    for source in outputs.iter().filter_map(|(_, source)| source.step()) {
        steps[source].output = true;
    }
    // A node that runs on the CPU too stays there when it would be alone on
    // the GPU, all it reads and all reading it on the CPU: two copies for
    // one step.
    for s in 0..steps.len() {
        let step = &steps[s];
        let on_cpu = |others: &[usize]| others.iter().all(|&o| steps[o].device == Device::Cpu);
        if step.device == Device::Cuda
            && step.node.descriptor().capabilities.cpu
            && on_cpu(&step.producers)
            && on_cpu(&step.readers)
        {
            steps[s].device = Device::Cpu;
        }
    }
    for step in &mut steps {
        let descriptor = step.node.descriptor();
        let mut hasher = DefaultHasher::new();
        (descriptor.id.as_str(), descriptor.version, step.device).hash(&mut hasher);
        hash_json(&step.params, &mut hasher);
        step.step.key().hash(&mut hasher);
        step.key = hasher.finish();
    }
    let transfers = transfers(recipe, &steps, &outputs);
    // The margin around a tile within which each step's outputs must be
    // right: what its readers need, and their halos.
    let mut margin = vec![0_u32; steps.len()];
    for s in (0..steps.len()).rev() {
        let needs = steps[s]
            .readers
            .iter()
            .map(|&r| margin[r].saturating_add(steps[r].halo));
        margin[s] = needs.max().unwrap_or(0);
    }
    let reads_input = |step: &Planned| {
        step.sources
            .iter()
            .flatten()
            .any(|s| matches!(s, Source::Input(_)))
    };
    let halos = steps
        .iter()
        .zip(&margin)
        .filter(|(step, _)| reads_input(step));
    let halo = halos
        .map(|(step, margin)| margin.saturating_add(step.halo))
        .max()
        .unwrap_or(0);
    Plan {
        name: recipe.name.clone(),
        inputs: recipe.inputs.clone(),
        steps,
        outputs,
        halo,
        transfers,
    }
}

/// The copies a plan makes: each image read on the other side of where it
/// is made, once for that side. Recipe inputs and outputs are in memory.
fn transfers(recipe: &Recipe, steps: &[Planned], outputs: &[(String, Source)]) -> usize {
    let sides = |source: Source, made: Device| {
        let read = steps
            .iter()
            .filter(|step| step.sources.contains(&Some(source)));
        let mut sides: HashSet<Device> = read.map(|step| step.device).collect();
        if outputs.iter().any(|(_, output)| *output == source) {
            sides.insert(Device::Cpu);
        }
        sides.into_iter().filter(|&side| side != made).count()
    };
    let images = recipe
        .inputs
        .iter()
        .enumerate()
        .filter(|(_, port)| port.kind == PortType::Image);
    let mut count: usize = images
        .map(|(i, _)| sides(Source::Input(i), Device::Cpu))
        .sum();
    for (s, step) in steps.iter().enumerate() {
        for (p, port) in step.node.descriptor().outputs.iter().enumerate() {
            if port.kind == PortType::Image {
                count += sides(Source::Step(s, p), step.device);
            }
        }
    }
    count
}
