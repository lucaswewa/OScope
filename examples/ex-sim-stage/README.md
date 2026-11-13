# ex-sim-stage

The simulated grblHAL stage, on the simulation clock (P11).

The stage moves as grblHAL does:

- **motion follows the clock:** a move of distance d at speed v takes d ÷ v, plus v ÷ a with acceleration a. A multi-axis move keeps its speed along the path;
- **homing** seeks each limit switch and pulls off it;
- **the trigger output** pulses as the stage passes armed positions;
- **status reports** are taken every 10 ms and arrive 5 ms later.

The example runs the clock 10 times faster than real time, so 23 simulated seconds take about 2.3 s. It homes, then makes four moves, the second passing three armed trigger positions. It prints each motion's planned duration, from its segments, and its measured one, until the first report after it ends. Then it jogs, stops, and serves a page that plots the stage's reported position against time.

## How to try it

```sh
cargo run --release -p ex-sim-stage
```

On the owner's PC it prints this, then `ex-sim-stage: open http://127.0.0.1:8217/`:

```text
the simulated stage at 10×, its motions planned and measured:
                                                 planned  measured    late
  home                                          15.663 s  15.670 s    7 ms
  to (20, 15) mm                                 2.600 s   2.608 s    8 ms
  to (80, 60) mm at 25 mm/s                      3.250 s   3.252 s    2 ms
  by 0.2 mm in x                                 0.089 s   0.095 s    5 ms
  down 5 mm in z                                 0.600 s   0.607 s    7 ms
  jog in -x at 5 mm/s, stopped after about 0.5 s: 3.193 mm in 0.795 s
  trigger pulse at x = 30.0 mm, at 19.0726 s
  trigger pulse at x = 45.0 mm, at 19.8226 s
  trigger pulse at x = 60.0 mm, at 20.5726 s
23.45 s simulated in 2.34 s on the host: 10.0×
```

**What the numbers show:**

- **Each motion is late by under 10 ms:** a move answers with the first report taken after it ends, and reports come every 10 ms.
- **The 0.2 mm move is short,** so it never reaches 10 mm/s: it accelerates to its middle and back, in 2 √(d ÷ a) = 89 ms.
- **The pulses are 0.75 s apart:** on the way to (80, 60) mm at 25 mm/s, x moves at 0.8 of the path's speed, 20 mm/s, so 15 mm in x takes 0.75 s.
- **The jog is stopped by the host,** not on a plan. It was asked to stop after 0.5 s, but the host's timer, run ten times faster, lets it go on a little. So it shows only how far it went.

**Open the address in a browser.** You should see:

- **x, y and z against simulated time,** with each motion shaded: homing, whose four legs show in the lines, the four moves, and the jog;
- **the three trigger pulses,** as dots on x;
- **a table of the motions,** planned against measured.

Ctrl-C stops the server.

**Options:**

- `--rate N`: the clock's speed, 10 by default;
- `--port N`: another port.

**`--smoke`** runs at 20×, on a port the system picks, with a temporary data root. Then it checks:

- that the page, its script and `results.json` are served;
- that homing and every move ended between 0 and 15 ms after their plan: a report interval, and a little for the time the host took to send the command, 20 times over;
- that the trigger pulsed at 30, 45 and 60 mm;
- that the reports cover the run.

```sh
cargo run -p ex-sim-stage -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
