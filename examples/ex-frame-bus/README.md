# ex-frame-bus

A synthetic MIChrome 20 feeding the frame bus (P03).

**The source:** 20 MP RGGB mosaics, 40 MB each, at 14.5 fps, filled into slabs from a pool of 24.

**The subscribers:**

- **`recorder`**, lossless. When its queue is full, the producer waits. It checks that it gets every frame, in order.
- **`preview`**, latest-wins and slow: 200 ms per frame. It drops most frames, and never slows the camera.
- **`statistics`**, latest-wins. It reads a 512 × 512 ROI of each frame it takes, through a view, without copying, at up to 10 Hz.

Every second it prints:
- the frame rate and throughput;
- each subscriber's delivered and dropped frames;
- how many of the pool's slabs are in use, and how often it ran out.

## How to try it

```sh
cargo run --release -p ex-frame-bus                 # 10 s
cargo run --release -p ex-frame-bus -- --seconds 60
cargo run -p ex-frame-bus -- --smoke                # 3 s, checked
```

It prints something like this:

```text
870 frames of 5472 x 3648 RGGB16 (39.9 MB each) at 14.5 fps, from a pool of 24
  9.1 s  14.52 fps     580 MB/s  recorder 132/0 dropped, preview 45/86 dropped, statistics 84/48 dropped  pool 1/24 in use, 0 exhausted
 ...
published 870 in 60.7 s (14.34 fps); recorder 870 in order: true; preview dropped 574; pool exhausted 0; last mean 25957
```

- **The recorder** has every frame, and drops none.
- **The preview** drops about two frames in three.
- **The pool** rarely has more than two slabs in use, because the slow subscribers hold only the newest frame.

The last line's rate is a little under 14.5, because it counts the time after the last frame too.

**`--smoke`** checks four things:
- the recorder got every frame, in order;
- the pool never ran out;
- the preview dropped frames;
- the rate stayed above 90% of 14.5 fps.

It ends with `smoke: ok`, or lists what failed and exits with code 1.

The pool takes about 1 GB of memory: 24 frames of 40 MB.
