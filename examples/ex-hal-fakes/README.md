# ex-hal-fakes

The hardware abstraction's scripted fakes, and its conformance suite (P09b).

**The fakes** stand in for each device in tests and examples:

- **the fake camera** streams frames of moving stripes from the pool it is given: at its mode's rate, or one for each software trigger;
- **the fake stage** reaches each target at once, and reports it on its position track;
- **the fake lights and objective mount** behave as the manual ones do. The mount says it is motorised.

Each has a **script**, which records what it was asked, and can make a chosen operation fail.

**The conformance suite** checks that a driver does what its trait promises (ADR-0012). The simulator runs it from P11, the real drivers in M3, and extension drivers in P27. For the camera it checks, among other things:

- every mode and format;
- what must be refused, and that a refusal changes nothing;
- that frames match their settings, in order;
- the ROI as the camera rounds it;
- frames lost to a full pool, counted;
- one frame for each software trigger;
- no frame after `stop`.

## How to try it

```sh
cargo run -p ex-hal-fakes
```

It prints, in turn:

1. **Each fake's capability report,** as JSON.
2. **Five frames from the camera,** in 16 bits:

   ```text
     frame 0: 640 × 480, mode full, exposed 1 ms from 0.023516700 s, its first row at 0
     frame 1: 640 × 480, mode full, exposed 1 ms from 0.046928000 s, its first row at 257
   ```

   The stripes move down a row each frame, so the first row's level rises by 257 each time, 1 in 8 bits. The times are since the clock started.
3. **The stage:** homed, moved to (10, 20, 5) mm, then moved by 1 mm in x and −0.5 mm in z. Then the track halfway in time between the two moves' reports, which gives the positions' mean, x 10500 µm and z 4750 µm.
4. **The lights and the objective.** The ring light, asked for 0.3, is set at 1, since it can't dim.
5. **The conformance suite** on each fake, then on a camera scripted to fail its `start`:

   ```text
   The conformance suite
     camera: passed
     stage: passed
     lights: passed
     objective mount: passed
     a camera whose start fails: caught at start streaming: it failed: the device isn't connected
   ```

   Last comes what that camera was asked, from its script.

**`--smoke`** also checks:

- the frames' numbers;
- the track's midpoint;
- the ring light at full intensity;
- that every fake passes the suite, and that the scripted failure is caught at `start streaming`.

```sh
cargo run -p ex-hal-fakes -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
