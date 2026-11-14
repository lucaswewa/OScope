//! Python extensions' workers: one process each, started on first use, kept
//! warm, started again after it dies, stopped when idle; and their nodes,
//! as the pipeline's (ADR-0020).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use oscope_compute::{Image, Shape};
use oscope_pipeline::{Descriptor, Inputs, Node, NodeError, Step, StepCtx, Value, Values};
use oscope_shm::Segment;
use serde_json::{Value as Json, json};

use crate::MAX_SAMPLES;
use crate::python::{PythonRuntime, interpreter};

/// The largest message from a worker: 64 MiB.
const MAX_MESSAGE: usize = 64 << 20;

/// The lines of a worker's stderr kept, to say why it died.
const KEPT_LINES: usize = 20;

/// Why a call to a worker gave no answer.
enum Failure {
    /// The node failed: the worker said why, and goes on.
    Node(String),
    /// The worker died, broke the protocol, or took too long: it is stopped.
    Worker(String),
}

/// A running worker process.
struct Process {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Result<Json, String>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    used: Instant,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A Python extension's worker (ADR-0020): one process, `python -m
/// oscope_ext.worker <entry>`, in the extension's folder and environment,
/// serving one call at a time.
pub(crate) struct Worker {
    id: String,
    folder: PathBuf,
    entry: String,
    start_timeout: Duration,
    call_timeout: Duration,
    idle: Duration,
    process: Mutex<Option<Process>>,
    /// Its process's ID, 0 when none runs: read without waiting for a call.
    pid: AtomicU32,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Worker {
    /// The worker of the extension `id` unpacked in `folder`, its module
    /// `entry`. It starts on first use; a thread stops it when idle.
    pub(crate) fn new(
        id: &str,
        folder: &Path,
        entry: &str,
        settings: &PythonRuntime,
    ) -> Arc<Worker> {
        let worker = Arc::new(Worker {
            id: id.to_owned(),
            folder: folder.to_owned(),
            entry: entry.to_owned(),
            start_timeout: settings.start_timeout,
            call_timeout: settings.call_timeout,
            idle: settings.idle,
            process: Mutex::new(None),
            pid: AtomicU32::new(0),
        });
        let watched = Arc::downgrade(&worker);
        std::thread::spawn(move || watch(watched));
        worker
    }

    /// The extension's ID.
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// Its process's ID, while it runs.
    pub(crate) fn pid(&self) -> Option<u32> {
        Some(self.pid.load(Ordering::Acquire)).filter(|pid| *pid != 0)
    }

    /// Takes the process out of `slot`, to stop it.
    fn take(&self, slot: &mut Option<Process>) -> Option<Process> {
        self.pid.store(0, Ordering::Release);
        slot.take()
    }

    /// Stops its process, if it runs.
    pub(crate) fn stop(&self) {
        self.take(&mut lock(&self.process));
    }

    /// The nodes its module declares.
    pub(crate) fn describe(&self) -> Result<Vec<Descriptor>, String> {
        let answer = self.with_process(|process, deadline| {
            process.send(&json!({ "describe": {} }))?;
            process.receive(deadline)
        })?;
        let nodes = answer
            .get("nodes")
            .cloned()
            .ok_or_else(|| format!("{}: no nodes in {answer}", self.id))?;
        serde_json::from_value(nodes).map_err(|e| format!("{}: its nodes: {e}", self.id))
    }

    /// Runs its node `node` on `images`, with `params`: its outputs, in
    /// order.
    pub(crate) fn run(
        &self,
        node: &str,
        params: &Json,
        images: &[&Image],
    ) -> Result<Vec<Image>, String> {
        self.with_process(|process, deadline| process.run(node, params, images, deadline))
    }

    /// Does `work` on the process, started first if it isn't running or
    /// has died, and stopped after if the worker failed.
    fn with_process<T>(
        &self,
        work: impl FnOnce(&mut Process, Instant) -> Result<T, Failure>,
    ) -> Result<T, String> {
        let mut slot = lock(&self.process);
        if slot
            .as_mut()
            .is_some_and(|process| matches!(process.child.try_wait(), Ok(Some(_))))
        {
            tracing::warn!("{}: its worker had died; starting another", self.id);
            self.take(&mut slot);
        }
        if slot.is_none() {
            let process = self.spawn().map_err(|why| format!("{}: {why}", self.id))?;
            self.pid.store(process.child.id(), Ordering::Release);
            *slot = Some(process);
        }
        let process = slot.as_mut().ok_or("no worker")?;
        let done = work(process, Instant::now() + self.call_timeout);
        process.used = Instant::now();
        match done {
            Ok(value) => Ok(value),
            Err(Failure::Node(why)) => Err(why),
            Err(Failure::Worker(why)) => {
                let said = self.take(&mut slot).map(|mut process| process.said());
                let said = said.unwrap_or_default();
                Err(format!("{}: {why}; its worker was stopped{said}", self.id))
            }
        }
    }

    /// Starts the process, and waits until it answers a ping.
    fn spawn(&self) -> Result<Process, String> {
        let python = interpreter(&self.folder);
        let mut child = Command::new(&python)
            .args(["-m", "oscope_ext.worker", &self.entry])
            .current_dir(&self.folder)
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("can't start {}: {e}", python.display()))?;
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdin), Some(stdout), Some(stderr)) = (stdin, stdout, stderr) else {
            return Err("its worker has no pipes".to_owned());
        };
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || read_messages(stdout, sender));
        let kept = Arc::new(Mutex::new(VecDeque::new()));
        let (id, lines) = (self.id.clone(), Arc::clone(&kept));
        std::thread::spawn(move || log_lines(&id, stderr, &lines));
        let mut process = Process {
            child,
            stdin,
            messages,
            stderr: kept,
            used: Instant::now(),
        };
        let deadline = Instant::now() + self.start_timeout;
        match process
            .send(&json!({ "ping": {} }))
            .and_then(|()| process.receive(deadline))
        {
            Ok(answer) if answer.get("pong").is_some() => {
                tracing::info!("{}: worker {} started", self.id, process.child.id());
                Ok(process)
            }
            Ok(answer) => Err(format!("its worker answered a ping with {answer}")),
            Err(Failure::Node(why) | Failure::Worker(why)) => {
                Err(format!("its worker didn't start: {why}{}", process.said()))
            }
        }
    }
}

/// Stops the worker's process once it has been idle long enough; ends with
/// the worker.
fn watch(worker: Weak<Worker>) {
    loop {
        let Some(worker) = worker.upgrade() else {
            return;
        };
        let tick = (worker.idle / 4).clamp(Duration::from_millis(50), Duration::from_secs(1));
        {
            let mut slot = lock(&worker.process);
            if slot
                .as_ref()
                .is_some_and(|process| process.used.elapsed() >= worker.idle)
            {
                worker.take(&mut slot);
                tracing::info!("{}: its idle worker was stopped", worker.id);
            }
        }
        drop(worker);
        std::thread::sleep(tick);
    }
}

/// Reads the worker's messages into `sender`, until it closes stdout.
fn read_messages(mut stdout: ChildStdout, sender: Sender<Result<Json, String>>) {
    loop {
        let mut head = [0; 4];
        if stdout.read_exact(&mut head).is_err() {
            let _ = sender.send(Err("it ended".to_owned()));
            return;
        }
        let length = u32::from_le_bytes(head) as usize;
        if length > MAX_MESSAGE {
            let _ = sender.send(Err(format!("it sent a message of {length} bytes")));
            return;
        }
        let mut body = vec![0; length];
        let message = match stdout.read_exact(&mut body) {
            Ok(()) => {
                serde_json::from_slice(&body).map_err(|e| format!("it sent what isn't JSON: {e}"))
            }
            Err(e) => Err(format!("it ended mid-message: {e}")),
        };
        let ended = message.is_err();
        if sender.send(message).is_err() || ended {
            return;
        }
    }
}

/// Logs the worker's stderr, line by line, keeping the last ones.
fn log_lines(id: &str, stderr: impl Read, kept: &Mutex<VecDeque<String>>) {
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        tracing::info!("{id}: {line}");
        let mut kept = lock(kept);
        if kept.len() == KEPT_LINES {
            kept.pop_front();
        }
        kept.push_back(line);
    }
}

/// A name for a new segment, unique in this process.
fn segment_name() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "py-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

impl Process {
    /// What it said last on stderr, and how it ended, to add to an error.
    fn said(&mut self) -> String {
        let lines: Vec<_> = lock(&self.stderr).iter().cloned().collect();
        let status = match self.child.try_wait() {
            Ok(Some(status)) => format!(" ({status})"),
            _ => String::new(),
        };
        match lines.is_empty() {
            true => status,
            false => format!("{status}:\n{}", lines.join("\n")),
        }
    }

    fn send(&mut self, message: &Json) -> Result<(), Failure> {
        let body = message.to_string();
        let sent = (self.stdin.write_all(&(body.len() as u32).to_le_bytes()))
            .and_then(|()| self.stdin.write_all(body.as_bytes()))
            .and_then(|()| self.stdin.flush());
        sent.map_err(|e| Failure::Worker(format!("can't write to it: {e}")))
    }

    fn receive(&mut self, deadline: Instant) -> Result<Json, Failure> {
        let wait = deadline.saturating_duration_since(Instant::now());
        match self.messages.recv_timeout(wait) {
            Ok(Ok(message)) => Ok(message),
            Ok(Err(why)) => Err(Failure::Worker(why)),
            Err(RecvTimeoutError::Timeout) => Err(Failure::Worker("it took too long".to_owned())),
            Err(RecvTimeoutError::Disconnected) => Err(Failure::Worker("it ended".to_owned())),
        }
    }

    /// Runs a node: each input copied into a segment; each output in a
    /// segment made when the worker asks, then copied out.
    fn run(
        &mut self,
        node: &str,
        params: &Json,
        images: &[&Image],
        deadline: Instant,
    ) -> Result<Vec<Image>, Failure> {
        let host = |why: String| Failure::Node(why);
        let mut inputs = Vec::new();
        let mut segments = Vec::new();
        for image in images {
            let mut segment = Segment::create(&segment_name(), 4 * image.data().len())
                .map_err(|e| host(e.to_string()))?;
            segment
                .write_f32(image.data())
                .map_err(|e| host(e.to_string()))?;
            let (width, height, channels) = (image.width(), image.height(), image.channels());
            inputs.push(json!({ "segment": segment.os_name(), "width": width, "height": height, "channels": channels }));
            segments.push(segment);
        }
        self.send(&json!({ "run": { "node": node, "params": params, "inputs": inputs } }))?;
        let mut outputs: Vec<(Segment, Shape)> = Vec::new();
        loop {
            let message = self.receive(deadline)?;
            if let Some(output) = message.get("output") {
                let size = |key: &str| {
                    output
                        .get(key)
                        .and_then(Json::as_u64)
                        .and_then(|n| u32::try_from(n).ok())
                };
                let (Some(width), Some(height), Some(channels)) =
                    (size("width"), size("height"), size("channels"))
                else {
                    return Err(Failure::Worker(format!(
                        "it asked for an output wrongly: {output}"
                    )));
                };
                let shape = Shape::new(width, height, channels as usize);
                if channels == 0 || shape.len() > MAX_SAMPLES {
                    self.send(&json!({ "error": "an output too large" }))?;
                    continue;
                }
                let segment = Segment::create(&segment_name(), 4 * shape.len())
                    .map_err(|e| host(e.to_string()))?;
                self.send(&json!({ "segment": segment.os_name() }))?;
                outputs.push((segment, shape));
            } else if message.get("done").is_some() {
                let read = |(segment, shape): (Segment, Shape)| {
                    let mut image = Image::zeros(shape);
                    segment
                        .read_f32(image.data_mut())
                        .map_err(|e| host(e.to_string()))?;
                    Ok(image)
                };
                return outputs.into_iter().map(read).collect();
            } else if let Some(error) = message.get("error") {
                let traceback = message
                    .get("traceback")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                tracing::info!("a Python node failed:\n{traceback}");
                return Err(Failure::Node(
                    error.as_str().unwrap_or("it failed").to_owned(),
                ));
            } else {
                return Err(Failure::Worker(format!("it sent {message}")));
            }
        }
    }
}

/// The nodes of a Python extension, as the pipeline's registry holds them.
pub(crate) fn nodes(worker: &Arc<Worker>, descriptors: &[Descriptor]) -> Vec<Arc<dyn Node>> {
    let node = |descriptor: &Descriptor| {
        let node = PythonNode {
            worker: Arc::clone(worker),
            descriptor: descriptor.clone(),
        };
        Arc::new(node) as Arc<dyn Node>
    };
    descriptors.iter().map(node).collect()
}

/// A node of a Python extension.
struct PythonNode {
    worker: Arc<Worker>,
    descriptor: Descriptor,
}

impl Node for PythonNode {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    fn prepare(&self, params: &Json) -> Result<Box<dyn Step>, String> {
        Ok(Box::new(PythonStep {
            worker: Arc::clone(&self.worker),
            descriptor: self.descriptor.clone(),
            params: params.clone(),
        }))
    }
}

/// A node of a Python extension, with its parameters.
struct PythonStep {
    worker: Arc<Worker>,
    descriptor: Descriptor,
    params: Json,
}

impl Step for PythonStep {
    fn run(&self, _: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        let ports = &self.descriptor.inputs;
        let images = ports.iter().map(|port| inputs.image(&port.name));
        let images = images.collect::<Result<Vec<&Image>, _>>()?;
        let made = self
            .worker
            .run(&self.descriptor.id, &self.params, &images)?;
        let outputs = &self.descriptor.outputs;
        if made.len() != outputs.len() {
            return Err(format!("it made {} outputs, of {}", made.len(), outputs.len()).into());
        }
        let named = outputs
            .iter()
            .zip(made)
            .map(|(port, image)| (port.name.clone(), Value::image(image)));
        Ok(named.collect())
    }
}
