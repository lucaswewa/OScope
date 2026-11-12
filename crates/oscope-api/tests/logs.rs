//! The logs: levels and critical, fields and jobs, queries and paging,
//! daily files and retention, the `logs` Thing, and the live tail.

use std::path::Path;
use std::time::Duration;

use chrono::{Days, Utc};
use futures_util::{SinkExt, StreamExt};
use oscope_api::logs::{EMIT_TARGET, Level, LogConfig, LogStore, Query, Record, prune};
use oscope_api::{Api, Roots};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tracing_subscriber::layer::SubscriberExt;

/// Records into `store` on this thread while the guard lives.
fn record_into(store: &LogStore) -> tracing::subscriber::DefaultGuard {
    tracing::subscriber::set_default(tracing_subscriber::registry().with(store.layer()))
}

/// Every record the store holds, oldest first.
fn all(store: &LogStore) -> Vec<Record> {
    let query = Query {
        limit: Some(1000),
        ..Query::default()
    };
    let mut records = store.query(&query).records;
    records.reverse();
    records
}

fn store(ring: usize, folder: Option<&Path>) -> LogStore {
    let config = LogConfig {
        ring,
        folder: folder.map(Path::to_owned),
        ..LogConfig::default()
    };
    LogStore::new(&config).unwrap()
}

#[test]
fn critical_is_an_error_marked_critical() {
    let store = LogStore::default();
    {
        let _recording = record_into(&store);
        tracing::error!(critical = true, "the stage hit its limit");
        tracing::error!("a frame was lost");
        tracing::warn!(critical = true, "a warning stays a warning");
        tracing::error!(critical = false, "not critical");
        tracing::info!("hello");
    }
    let records = all(&store);
    let levels: Vec<Level> = records.iter().map(|r| r.level).collect();
    use Level::*;
    assert_eq!(levels, [Critical, Error, Warn, Error, Info]);
    assert!(!records[0].fields.contains_key("critical"));
    assert_eq!(records[2].fields["critical"], true);
    assert_eq!(records[3].fields["critical"], false);
    assert!(Trace < Debug && Debug < Info && Info < Warn && Warn < Error && Error < Critical);
    assert_eq!(serde_json::to_value(Critical).unwrap(), "critical");
}

#[test]
fn records_carry_their_fields_target_and_job() {
    let store = LogStore::default();
    let capture = store.capture("job-7");
    {
        let _recording = record_into(&store);
        tracing::info!(target: "oscope::stage", x_um = 12.5, steps = 40u64, homed = true, axis = "x", "moved");
        tracing::info_span!("job", job_id = %"job-7").in_scope(|| {
            tracing::info!("in the job");
            tracing::info_span!("step").in_scope(|| tracing::warn!("deeper in the job"));
        });
        tracing::info_span!("job", job_id = %"job-8").in_scope(|| tracing::info!("another job"));
        tracing::info!("after the jobs");
    }
    let records = all(&store);
    let first = &records[0];
    assert_eq!(
        (first.target.as_str(), first.message.as_str()),
        ("oscope::stage", "moved")
    );
    let expected = json!({"x_um": 12.5, "steps": 40, "homed": true, "axis": "x"});
    assert_eq!(Value::Object(first.fields.clone()), expected);
    let jobs: Vec<Option<&str>> = records.iter().map(|r| r.job.as_deref()).collect();
    assert_eq!(
        jobs,
        [None, Some("job-7"), Some("job-7"), Some("job-8"), None]
    );
    let numbers: Vec<u64> = records.iter().map(|r| r.seq).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 5]);
    assert!(records.windows(2).all(|pair| pair[0].time <= pair[1].time));

    let captured: Vec<String> = capture.records().into_iter().map(|r| r.message).collect();
    assert_eq!(captured, ["in the job", "deeper in the job"]);
}

#[test]
fn a_capture_keeps_a_jobs_records_however_small_the_ring() {
    let store = store(2, None);
    let capture = store.capture("long");
    {
        let _recording = record_into(&store);
        for i in 0..20 {
            let job = if i % 2 == 0 { "long" } else { "other" };
            tracing::info_span!("job", job_id = job).in_scope(|| tracing::info!(i, "step"));
        }
    }
    let steps: Vec<Value> = capture
        .records()
        .iter()
        .map(|r| r.fields["i"].clone())
        .collect();
    assert_eq!(
        steps,
        (0..20).step_by(2).map(Value::from).collect::<Vec<_>>()
    );
    assert_eq!(all(&store).len(), 2);
}

#[test]
fn queries_filter_by_level_time_target_text_and_job_newest_first_in_pages() {
    let store = store(1000, None);
    {
        let _recording = record_into(&store);
        for i in 0..300 {
            let stage = if i % 7 == 0 { "Stage X" } else { "camera" };
            macro_rules! at {
                ($target:literal) => {
                    match i % 5 {
                        0 => tracing::trace!(target: $target, stage, "record {i}"),
                        1 => tracing::debug!(target: $target, stage, "record {i}"),
                        2 => tracing::info!(target: $target, stage, "record {i}"),
                        3 => tracing::warn!(target: $target, stage, "record {i}"),
                        _ => tracing::error!(target: $target, stage, "record {i}"),
                    }
                };
            }
            if i % 2 == 0 {
                at!("oscope::a")
            } else {
                at!("other::b")
            }
        }
    }
    let everything = all(&store);
    let count = |query: Query| {
        let query = Query {
            limit: Some(1000),
            ..query
        };
        let found = store.query(&query).records;
        let expected = everything.iter().filter(|r| query.matches(r)).count();
        assert_eq!(found.len(), expected, "{query:?}");
        found.len()
    };

    let first = store.query(&Query::default());
    assert_eq!(first.records.len(), 100);
    assert_eq!((first.records[0].seq, first.records[99].seq), (300, 201));
    assert_eq!(first.next, Some(201));

    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = store.query(&Query {
            before,
            limit: Some(64),
            ..Query::default()
        });
        seen.extend(page.records.iter().map(|r| r.seq));
        match page.next {
            Some(next) => before = Some(next),
            None => break,
        }
    }
    assert_eq!(seen, (1..=300).rev().collect::<Vec<u64>>());

    assert_eq!(
        count(Query {
            level: Some(Level::Warn),
            ..Query::default()
        }),
        120
    );
    assert_eq!(
        count(Query {
            target: Some("oscope".into()),
            ..Query::default()
        }),
        150
    );
    assert_eq!(
        count(Query {
            text: Some("STAGE x".into()),
            ..Query::default()
        }),
        43
    );
    assert_eq!(
        count(Query {
            text: Some("record 29".into()),
            ..Query::default()
        }),
        11
    );
    let (since, until) = (everything[100].time, everything[200].time);
    let window = count(Query {
        since: Some(since),
        until: Some(until),
        ..Query::default()
    });
    assert!(window >= 100, "{window}");
    let combined = Query {
        level: Some(Level::Error),
        target: Some("other".into()),
        text: Some("stage x".into()),
        ..Query::default()
    };
    assert!(count(combined) > 0);
    assert_eq!(
        count(Query {
            job: Some("none".into()),
            ..Query::default()
        }),
        0
    );
    assert_eq!(
        Query {
            limit: Some(0),
            ..Query::default()
        }
        .page_size(),
        1
    );
    assert_eq!(
        Query {
            limit: Some(5000),
            ..Query::default()
        }
        .page_size(),
        1000
    );
}

#[test]
fn the_ring_keeps_the_newest_records() {
    let store = store(50, None);
    {
        let _recording = record_into(&store);
        for i in 0..120 {
            tracing::info!(i, "record");
        }
    }
    let numbers: Vec<u64> = all(&store).iter().map(|r| r.seq).collect();
    assert_eq!(numbers, (71..=120).collect::<Vec<u64>>());
}

#[test]
fn daily_files_hold_json_lines_and_are_kept_30_days() {
    let folder = tempfile::tempdir().unwrap();
    let today = Utc::now().date_naive();
    let day = |ago: u64| today - Days::new(ago);
    for ago in [40, 31, 30, 29, 1] {
        std::fs::write(folder.path().join(format!("oscope-{}.jsonl", day(ago))), "").unwrap();
    }
    for other in ["notes.txt", "oscope-garbage.jsonl"] {
        std::fs::write(folder.path().join(other), "keep me").unwrap();
    }

    let store = store(10, Some(folder.path()));
    let kept: Vec<String> = store.files().unwrap().into_iter().map(|f| f.name).collect();
    let names = |days: &[u64]| {
        days.iter()
            .map(|&ago| format!("oscope-{}.jsonl", day(ago)))
            .collect::<Vec<_>>()
    };
    assert_eq!(kept, names(&[1, 29]));
    assert!(folder.path().join("notes.txt").exists());
    assert!(folder.path().join("oscope-garbage.jsonl").exists());

    {
        let _recording = record_into(&store);
        tracing::info!(n = 1, "one");
        tracing::warn!("two");
        tracing::error!(critical = true, "three");
    }
    let files = store.files().unwrap();
    assert_eq!(files[0].name, names(&[0])[0]);
    let text = std::fs::read_to_string(store.file(&files[0].name).unwrap()).unwrap();
    assert_eq!(files[0].bytes, text.len() as u64);
    let written: Vec<Record> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(written, all(&store));

    assert_eq!(store.file("notes.txt"), None);
    assert_eq!(store.file("../oscope-2026-01-01.jsonl"), None);

    // Two days on, the file from 29 days ago is 31 days old.
    let deleted = prune(folder.path(), today + Days::new(2), 30).unwrap();
    assert_eq!(deleted, [folder.path().join(&names(&[29])[0])]);
    let deleted = prune(folder.path(), today, 1).unwrap();
    assert_eq!(deleted, [folder.path().join(&names(&[1])[0])]);
    assert_eq!(store.files().unwrap().len(), 1);
}

fn roots(folder: &Path) -> Roots {
    Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    }
}

#[tokio::test]
async fn the_logs_thing_queries_lists_downloads_and_emits() {
    let folder = tempfile::tempdir().unwrap();
    let store = store(1000, Some(&folder.path().join("logs")));
    let _recording = record_into(&store);
    let api = Api::builder(&roots(folder.path()))
        .logs(store.clone())
        .build()
        .unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();

    let emitted = client
        .post_json(
            "/api/v1/logs/emit",
            Some(&json!({"level": "critical", "message": "Slide loaded"})),
        )
        .await;
    assert_eq!(emitted.status, 201, "{}", emitted.text());
    let wanted = "/api/v1/logs/records?level=critical&text=slide";
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let page = loop {
        let page = client.get(wanted).await.json();
        if !page["records"].as_array().unwrap().is_empty() {
            break page;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the record never came"
        );
        tokio::task::yield_now().await;
    };
    let record = &page["records"][0];
    assert_eq!(
        (record["target"].as_str(), record["message"].as_str()),
        (Some(EMIT_TARGET), Some("Slide loaded"))
    );

    tracing::info!("one more");
    let first = client.get("/api/v1/logs/records?limit=1").await.json();
    assert_eq!(first["records"].as_array().unwrap().len(), 1);
    let next = first["next"].as_u64().unwrap();
    let older = client
        .get(&format!("/api/v1/logs/records?limit=1&before={next}"))
        .await
        .json();
    assert!(older["records"][0]["seq"].as_u64().unwrap() < next);
    for wrong in ["level=loud", "limit=many", "colour=red"] {
        let answer = client.get(&format!("/api/v1/logs/records?{wrong}")).await;
        assert_eq!(answer.status, 400, "{wrong}");
    }

    let files = client.get("/api/v1/logs/files").await.json();
    let name = files[0]["name"].as_str().unwrap().to_owned();
    assert_eq!(name, format!("oscope-{}.jsonl", Utc::now().date_naive()));
    let download = client.get(&format!("/api/v1/logs/files/{name}")).await;
    assert_eq!(download.status, 200);
    assert_eq!(
        download.header("content-type"),
        Some("application/x-ndjson")
    );
    assert!(
        download
            .header("content-disposition")
            .unwrap()
            .contains(&name)
    );
    assert!(download.text().contains("Slide loaded"));
    for missing in ["notes.txt", "..%2Foscope-2026-01-01.jsonl"] {
        assert_eq!(
            client
                .get(&format!("/api/v1/logs/files/{missing}"))
                .await
                .status,
            404
        );
    }

    let td = client.get("/api/v1/logs/").await.json();
    assert!(td["properties"]["files"]["readOnly"] == true && td["actions"]["emit"].is_object());
}

/// The next JSON message from the tail, within 10 s.
async fn next<S>(socket: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let message = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    let message = message.expect("a message in time").unwrap().unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}

#[tokio::test]
async fn the_live_tail_sends_the_backlog_then_new_records_in_order() {
    let folder = tempfile::tempdir().unwrap();
    let store = store(5000, None);
    let _recording = record_into(&store);
    for i in 0..3 {
        tracing::info!(target: "tail", "before {i}");
    }
    let api = Api::builder(&roots(folder.path()))
        .logs(store.clone())
        .build()
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(api.serve(listener, async {
        let _ = stopped.await;
    }));
    let url = format!("ws://{address}/api/v1/media/logs");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();

    // The query it follows, the backlog, oldest first, then the mark.
    assert!(next(&mut socket).await["query"].is_object());
    let mut before = Vec::new();
    let sent = loop {
        let message = next(&mut socket).await;
        if let Some(sent) = message.get("caught_up") {
            break sent.as_u64().unwrap();
        }
        if message["record"]["target"] == "tail" {
            before.push(message["record"]["message"].as_str().unwrap().to_owned());
        }
    };
    assert_eq!(before, ["before 0", "before 1", "before 2"]);
    assert!(sent >= 3);

    let query = json!({"target": "tail", "level": "warn", "limit": 10});
    socket.send(Message::text(query.to_string())).await.unwrap();
    let followed = next(&mut socket).await;
    assert_eq!(followed["query"]["target"], "tail", "{followed}");
    assert_eq!(next(&mut socket).await, json!({"caught_up": 0}));
    for i in 0..30 {
        match i % 3 {
            0 => tracing::info!(target: "tail", "live {i}"),
            1 => tracing::warn!(target: "tail", "live {i}"),
            _ => tracing::error!(target: "elsewhere", "live {i}"),
        }
    }
    for i in (1..30).step_by(3) {
        let message = next(&mut socket).await;
        assert_eq!(
            message["record"]["message"],
            format!("live {i}"),
            "{message}"
        );
    }

    // A follower that falls behind is told how many it missed.
    for i in 0..2000 {
        tracing::warn!(target: "tail", "flood {i}");
    }
    let (mut received, mut skipped, mut last) = (0, 0, -1_i64);
    while received + skipped < 2000 {
        let message = next(&mut socket).await;
        if let Some(n) = message.get("skipped") {
            skipped += n.as_u64().unwrap();
            continue;
        }
        let text = message["record"]["message"].as_str().unwrap();
        let i: i64 = text.strip_prefix("flood ").unwrap().parse().unwrap();
        assert!(i > last, "{i} after {last}");
        (received, last) = (received + 1, i);
    }
    assert!(
        skipped > 0 && last == 1999,
        "{skipped} skipped, last {last}"
    );

    socket.send(Message::text("{\"colour\": 1}")).await.unwrap();
    assert!(next(&mut socket).await["error"].is_string());

    stop.send(()).unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    match closed {
        Some(Ok(Message::Close(Some(frame)))) => assert_eq!(frame.code, CloseCode::Away),
        other => panic!("expected a close frame, got {other:?}"),
    }
    serving.await.unwrap().unwrap();
}
