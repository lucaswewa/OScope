//! The job service: hardware jobs in order and one at a time, holding the
//! lock; the compute pool's workers and GPU budget; cancelling, pausing,
//! progress and failures; and the history across a restart.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oscope_core::JobId;
use oscope_jobs::{
    Job, JobCtx, JobError, JobFilter, JobFuture, JobService, JobSpec, JobState, JobsConfig,
    Resource, Stop,
};
use serde_json::json;
use tokio::sync::Notify;

fn spec(resource: Resource, gpu_mib: u32, pausable: bool) -> JobSpec {
    JobSpec {
        kind: "test".to_owned(),
        title: "a test job".to_owned(),
        account: None,
        params: json!({}),
        resource,
        pausable,
        gpu_mib,
    }
}

fn job(
    resource: Resource,
    gpu_mib: u32,
    pausable: bool,
    code: impl FnOnce(JobCtx) -> JobFuture + Send + 'static,
) -> Job {
    Job {
        spec: spec(resource, gpu_mib, pausable),
        run: Box::new(code),
    }
}

fn open(folder: &tempfile::TempDir, config: JobsConfig) -> JobService {
    JobService::open(&folder.path().join("index.sqlite"), config).unwrap()
}

/// Waits up to 10 s for the job `id` to be in `state`.
async fn wait_for(service: &JobService, id: JobId, state: JobState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = service.get(id).unwrap().state;
        if now == state {
            return;
        }
        assert!(Instant::now() < deadline, "{id} is {now:?}, not {state:?}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hardware_jobs_run_one_at_a_time_in_their_queue_s_order() {
    let folder = tempfile::tempdir().unwrap();
    let service = open(&folder, JobsConfig::default());
    let (log, gate) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Notify::new()));
    let hardware = service.hardware();
    let step = |name: &'static str, gate: Option<Arc<Notify>>| {
        let (log, hardware) = (Arc::clone(&log), hardware.clone());
        job(Resource::Hardware, 0, false, move |ctx| {
            Box::pin(async move {
                // It holds the lock while it runs.
                assert_eq!(hardware.holder(), Some(ctx.id()));
                log.lock().unwrap().push((name, "starts"));
                if let Some(gate) = gate {
                    gate.notified().await;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
                log.lock().unwrap().push((name, "ends"));
                Ok(())
            })
        })
    };
    // The first waits for the gate, so that the others queue behind it.
    let first = service
        .submit(step("first", Some(Arc::clone(&gate))))
        .unwrap();
    wait_for(&service, first, JobState::Running).await;
    let ids: Vec<JobId> = ["second", "third", "fourth"]
        .into_iter()
        .map(|name| service.submit(step(name, None)).unwrap())
        .collect();
    assert_eq!(service.get(ids[2]).unwrap().position, Some(2));
    // The fourth to the front; the running one can't move.
    service.reorder(ids[2], 0).unwrap();
    assert_eq!(service.get(ids[2]).unwrap().position, Some(0));
    assert_eq!(service.get(ids[0]).unwrap().position, Some(1));
    assert!(matches!(
        service.reorder(first, 0),
        Err(JobError::State { .. })
    ));
    gate.notify_one();
    wait_for(&service, ids[1], JobState::Completed).await;
    assert_eq!(hardware.holder(), None);
    let names = ["first", "fourth", "second", "third"];
    let expected: Vec<_> = names
        .iter()
        .flat_map(|n| [(*n, "starts"), (*n, "ends")])
        .collect();
    assert_eq!(
        *log.lock().unwrap(),
        expected,
        "in order, never two at once"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn compute_jobs_share_the_workers_and_the_gpu_budget() {
    let folder = tempfile::tempdir().unwrap();
    let config = JobsConfig {
        workers: 2,
        gpu_mib: Some(1000),
        ..JobsConfig::default()
    };
    let service = open(&folder, config);
    // How many run at once, at most, for jobs needing `gpu_mib` each.
    let most_at_once = |gpu_mib: u32| {
        let (now, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let ids: Vec<JobId> = (0..4)
            .map(|_| {
                let (now, most) = (Arc::clone(&now), Arc::clone(&most));
                let code = move |_: JobCtx| -> JobFuture {
                    Box::pin(async move {
                        let running = now.fetch_add(1, Ordering::SeqCst) + 1;
                        most.fetch_max(running, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        now.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                };
                service
                    .submit(job(Resource::Compute, gpu_mib, false, code))
                    .unwrap()
            })
            .collect();
        (ids, most)
    };
    for (gpu_mib, expected) in [(0, 2), (600, 1), (400, 2)] {
        let (ids, most) = most_at_once(gpu_mib);
        for id in ids {
            wait_for(&service, id, JobState::Completed).await;
        }
        assert_eq!(most.load(Ordering::SeqCst), expected, "{gpu_mib} MiB each");
    }
    let greedy = job(Resource::Compute, 1001, false, |_| {
        Box::pin(async { Ok(()) })
    });
    assert!(matches!(service.submit(greedy), Err(JobError::Refused(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_job_stops_at_its_checkpoint_and_a_queued_one_never_starts() {
    let folder = tempfile::tempdir().unwrap();
    let service = open(&folder, JobsConfig::default());
    let (steps, past_the_end) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicBool::new(false)),
    );
    let (counted, ended) = (Arc::clone(&steps), Arc::clone(&past_the_end));
    let long = service
        .submit(job(Resource::Hardware, 0, false, move |ctx| {
            Box::pin(async move {
                for _ in 0..10_000 {
                    ctx.checkpoint().await?;
                    counted.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                ended.store(true, Ordering::SeqCst);
                Ok(())
            })
        }))
        .unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&started);
    let queued = service
        .submit(job(Resource::Hardware, 0, false, move |_| {
            Box::pin(async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
        }))
        .unwrap();
    wait_for(&service, long, JobState::Running).await;
    service.cancel(queued).unwrap();
    assert_eq!(service.get(queued).unwrap().state, JobState::Cancelled);
    service.cancel(long).unwrap();
    wait_for(&service, long, JobState::Cancelled).await;
    let done = steps.load(Ordering::SeqCst);
    assert!(
        done < 10_000 && !past_the_end.load(Ordering::SeqCst),
        "{done} steps"
    );
    assert!(!started.load(Ordering::SeqCst), "the queued job ran");
    assert!(service.get(long).unwrap().finished.is_some());
    assert!(matches!(service.cancel(long), Err(JobError::State { .. })));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_job_waits_at_its_checkpoint_until_resumed() {
    let folder = tempfile::tempdir().unwrap();
    let service = open(&folder, JobsConfig::default());
    let (steps, go) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicBool::new(false)),
    );
    let (counted, finish) = (Arc::clone(&steps), Arc::clone(&go));
    let id = service
        .submit(job(Resource::Compute, 0, true, move |ctx| {
            Box::pin(async move {
                while !finish.load(Ordering::SeqCst) {
                    ctx.checkpoint().await?;
                    counted.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                Ok(())
            })
        }))
        .unwrap();
    wait_for(&service, id, JobState::Running).await;
    service.pause(id).unwrap();
    wait_for(&service, id, JobState::Paused).await;
    // Paused, it makes no progress, even after it is told to finish.
    go.store(true, Ordering::SeqCst);
    let held = steps.load(Ordering::SeqCst);
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(steps.load(Ordering::SeqCst), held);
    service.resume(id).unwrap();
    wait_for(&service, id, JobState::Completed).await;

    // A job that can't pause, and one that isn't running, are refused.
    let fixed = service
        .submit(job(Resource::Hardware, 0, false, |ctx| {
            Box::pin(async move {
                ctx.cancelled().await;
                Err(Stop::Cancelled)
            })
        }))
        .unwrap();
    wait_for(&service, fixed, JobState::Running).await;
    assert_eq!(service.pause(fixed), Err(JobError::NotPausable(fixed)));
    assert!(matches!(service.resume(id), Err(JobError::State { .. })));
    service.cancel(fixed).unwrap();
    wait_for(&service, fixed, JobState::Cancelled).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_results_and_failures_are_reported() {
    let folder = tempfile::tempdir().unwrap();
    let service = open(&folder, JobsConfig::default());
    let mut events = service.subscribe();
    let gate = Arc::new(Notify::new());
    let held = Arc::clone(&gate);
    let id = service
        .submit(job(Resource::Compute, 0, false, move |ctx| {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                ctx.progress(Some(0.5), "halfway");
                ctx.count("tiles", 12);
                ctx.add_result("result-1");
                held.notified().await;
                Ok(())
            })
        }))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let halfway = loop {
        let info = tokio::time::timeout_at(deadline.into(), events.recv())
            .await
            .unwrap()
            .unwrap();
        if info.id == id && info.results == ["result-1"] {
            break info;
        }
    };
    assert_eq!(halfway.progress.fraction, Some(0.5));
    assert_eq!(halfway.progress.stage, "halfway");
    assert_eq!(halfway.progress.counters["tiles"], 12);
    let eta = halfway.progress.eta_s.unwrap();
    assert!(eta > 0.0 && eta < 5.0, "{eta}");
    gate.notify_one();
    wait_for(&service, id, JobState::Completed).await;

    let failing = |panics: bool| {
        job(Resource::Compute, 0, false, move |_| {
            Box::pin(async move {
                if panics {
                    panic!("on purpose");
                }
                Err(Stop::failed("the stage didn't answer"))
            })
        })
    };
    for (panics, reason) in [(false, "the stage didn't answer"), (true, "panicked")] {
        let id = service.submit(failing(panics)).unwrap();
        wait_for(&service, id, JobState::Failed).await;
        let error = service.get(id).unwrap().error.unwrap();
        assert!(error.contains(reason), "{error}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_history_survives_a_restart_and_unfinished_jobs_are_interrupted() {
    let folder = tempfile::tempdir().unwrap();
    // The first server, on a runtime of its own, which then stops.
    let path = folder.path().join("index.sqlite");
    let ids = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let service = JobService::open(&path, JobsConfig::default()).unwrap();
            let quick = service
                .submit(job(Resource::Compute, 0, false, |ctx| {
                    Box::pin(async move {
                        ctx.add_result("made");
                        Ok(())
                    })
                }))
                .unwrap();
            wait_for(&service, quick, JobState::Completed).await;
            let forever = || {
                job(Resource::Hardware, 0, false, |ctx| {
                    Box::pin(async move {
                        ctx.cancelled().await;
                        Err(Stop::Cancelled)
                    })
                })
            };
            let running = service.submit(forever()).unwrap();
            let queued = service.submit(forever()).unwrap();
            wait_for(&service, running, JobState::Running).await;
            (quick, running, queued)
        })
        // The runtime, and the server with it, stops here.
    })
    .join()
    .unwrap();
    let (quick, running, queued) = ids;

    let service = open(&folder, JobsConfig::default());
    let done = service.get(quick).unwrap();
    assert_eq!(
        (done.state, done.results),
        (JobState::Completed, vec!["made".to_owned()])
    );
    for (id, was) in [(running, "running"), (queued, "queued")] {
        let job = service.get(id).unwrap();
        assert_eq!(job.state, JobState::Interrupted);
        assert!(job.finished.is_some() && job.position.is_none());
        assert_eq!(
            job.error.unwrap(),
            format!("the server stopped while it was {was}")
        );
    }
    // Lists: the newest first, filtered.
    let all = service.list(&JobFilter::default()).unwrap();
    assert_eq!(
        all.iter().map(|j| j.id).collect::<Vec<_>>(),
        [queued, running, quick]
    );
    let filter = JobFilter {
        state: Some(JobState::Interrupted),
        limit: Some(1),
        ..JobFilter::default()
    };
    assert_eq!(
        service
            .list(&filter)
            .unwrap()
            .iter()
            .map(|j| j.id)
            .collect::<Vec<_>>(),
        [queued]
    );
    let other = JobFilter {
        kind: Some("other".to_owned()),
        ..JobFilter::default()
    };
    assert!(service.list(&other).unwrap().is_empty());
    let unknown = JobId::generate();
    assert_eq!(service.get(unknown), Err(JobError::Unknown(unknown)));
}

#[tokio::test(flavor = "multi_thread")]
async fn kinds_make_jobs_from_their_parameters() {
    let folder = tempfile::tempdir().unwrap();
    let service = open(&folder, JobsConfig::default());
    service.register(
        "demo.count",
        Arc::new(|params| {
            let to = params["to"].as_u64().ok_or("`to` is a number")?;
            Ok(job(Resource::Compute, 0, false, move |ctx| {
                Box::pin(async move {
                    ctx.count("counted", to);
                    Ok(())
                })
            }))
        }),
    );
    assert_eq!(service.kinds(), ["demo.count"]);
    let id = service
        .submit_kind("demo.count", json!({"to": 3}), Some("ada".to_owned()))
        .unwrap();
    wait_for(&service, id, JobState::Completed).await;
    let info = service.get(id).unwrap();
    assert_eq!(info.spec.kind, "demo.count");
    assert_eq!(info.spec.account.as_deref(), Some("ada"));
    assert_eq!(info.spec.params, json!({"to": 3}));
    assert_eq!(info.progress.counters["counted"], 3);
    let bad = service.submit_kind("demo.count", json!({}), None);
    assert!(matches!(bad, Err(JobError::Refused(reason)) if reason.contains("`to`")));
    let unknown = service.submit_kind("demo.nothing", json!({}), None);
    assert!(matches!(unknown, Err(JobError::Refused(_))));
}
