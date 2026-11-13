# ex-jobs

The job service (P20, [ADR-0016](../../docs/adr/0016-the-job-service.md)): every long piece of work queued, followed, controlled and recorded.

- **Hardware jobs** run one at a time, and hold the hardware lock while they run: the stage refuses manual moves meanwhile.
- **Compute jobs** run on a pool of workers, two here, each holding the GPU memory it asks for, from a budget of 2048 MiB here.

The example registers two kinds of job:

- **`demo.tour`,** a hardware job: the simulated stage visits `points` stops along x, `step_um` apart, then goes back to the start.
- **`demo.crunch`,** a compute job: `seconds` of work in 50 ms steps, holding `gpu_mib` of the GPU budget.

Both can pause, at each stop or step.

## How to try it

```sh
cargo run -p ex-jobs
```

It starts the simulated microscope on the scaled-down sensor, homes the stage and places it over the H&E slide, then prints `ex-jobs: open http://127.0.0.1:8211/`.

Port 8211 is `ex-server-hello`'s too: the range for manual checks is used up. Run one at a time.

**The page:**

- **Tour** and **Crunch** submit jobs, with their parameters.
- **The queue** shows each job not yet ended, live: its state, its place in its queue, and a progress bar with what it is doing and the time it should take still.
  - **Pause** and **Resume**, for a running job.
  - **First**, for a queued tour.
  - **Cancel**: a queued job at once, a running one at its next stop or step.
- **The history** shows the jobs that ended, with their results, or why they failed.
- **The hardware lock** says which job holds it.
- **Jog the stage by hand** (hold it down) is refused while a tour runs, with the job that holds the hardware.

**Things to try:**

- **Several tours:** they run one after the other. Move the last to the front with **First**.
- **Two crunches with 1500 MiB each:** the budget is 2048, so they run one after the other. With 1000 each, they run together.
- **Pause a tour,** then jog by hand: still refused, as the tour still holds the hardware.
- **Stop the example during a tour,** and start it again: the history shows the tour *interrupted*.

**With `curl`:**

```sh
curl -X POST -H "Content-Type: application/json" \
     -d '{"kind": "demo.tour", "params": {"points": 3}}' http://127.0.0.1:8211/api/v1/jobs/submit
# The invocation's output is the job's JobRef: {"id": …, "href": "/api/v1/jobs/job/…"}
curl http://127.0.0.1:8211/api/v1/jobs/queue
curl "http://127.0.0.1:8211/api/v1/jobs/list?state=completed&limit=5"
curl -N http://127.0.0.1:8211/api/v1/jobs/changed            # the changes, as server-sent events
curl "http://127.0.0.1:8211/api/v1/logs/records?job=<id>"    # a job's own log
```

**The history** is in `.oscope/ex-jobs/data/index.sqlite`.

**`--smoke`** checks it with a temporary data root:

- that the page is served;
- that a tour holds the hardware, and a manual jog is refused meanwhile;
- that two crunches with 1500 MiB each run one after the other;
- that all three complete, and are listed.

```sh
cargo run -p ex-jobs -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
