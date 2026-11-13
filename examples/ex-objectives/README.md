# ex-objectives

The objective catalogue and the manual implementations of the hardware abstraction (P09).

**The catalogue.** For each objective, the example prints what it gives on the MIChrome 20, whose sensor is 5472 × 3648 pixels of 2.4 µm:

- **the pixel at the sample:** the sensor's pixel divided by the objective's magnification on this instrument. That magnification is the objective's own, scaled by the instrument's tube lens against the one the objective was designed for;
- **the field of view:** the sensor, at that pixel size;
- **the resolution:** Rayleigh's criterion, 0.61 λ / NA, in green light (λ = 0.55 µm).

By default it uses the catalogue built into `oscope-hal`, Mitutoyo's published values, which [`configs/objectives.toml`](../../configs/objectives.toml) repeats. `--catalogue FILE` reads another.

**The manual implementations,** each used through its trait, as the server will use it:

- **the manual objective mount:** the user swaps objectives by hand and says which one is fitted. It refuses an objective that isn't in the catalogue, and keeps the one it had;
- **the manual lights:** the user switches them, and OScope records what they say. They refuse an intensity outside 0–1, and keep what they had.

Each prints its capability report, as JSON.

## How to try it

```sh
cargo run -p ex-objectives
```

You should see:

```text
The objective catalogue: Mitutoyo's defaults, behind a 200 mm tube lens
On the MIChrome 20, 5472 × 3648 pixels of 2.4 µm, in light of 0.55 µm:

  name      NA   working       pixel    field of view  resolution  model
  5x      0.14   34.0 mm    0.480 µm   2.63 × 1.75 mm     2.40 µm  Mitutoyo M Plan Apo 5×
  10x     0.28   33.5 mm    0.240 µm   1.31 × 0.88 mm     1.20 µm  Mitutoyo M Plan Apo 10×
  20x     0.42   20.0 mm    0.120 µm   0.66 × 0.44 mm     0.80 µm  Mitutoyo M Plan Apo 20×
  50x     0.55   13.0 mm    0.048 µm   0.26 × 0.18 mm     0.61 µm  Mitutoyo M Plan Apo 50×
```

At 50× the pixel, 0.048 µm, is far finer than the resolution, 0.61 µm: the optics limit what is seen there, not the sensor (plan §5.5).

Then the mount and the lights:

```text
The manual objective mount: the user swaps objectives by hand, and says which
  capabilities: {"model":"manual","motorised":false}
  fitted "20x": Mitutoyo M Plan Apo 20×
  fitting "7x": there is no objective named "7x"
  fitted now: Some("20x")

The manual lights: the user switches them, and OScope records what they say
  capabilities: {"model":"manual","channels":[{"name":"transmitted","kind":"transmitted","dimmable":true}]}
  setting an intensity of 1.5: the intensity must be from 0 to 1, and was 1.5
  state: [{"channel":"transmitted","on":true,"intensity":0.6}]
```

**Another catalogue.** Copy `configs/objectives.toml`, change it, and pass it with `--catalogue`. Lengths are in micrometres. A tube lens of 180000 (180 mm), for example, makes the 20× magnify 18×, and its pixel 0.133 µm. A file that breaks a rule is refused with the rule, such as `"20x"'s numerical aperture is 0`, and the example exits with code 1.

```sh
cargo run -p ex-objectives -- --catalogue my-objectives.toml
```

**`--smoke`** also checks:

- that `configs/objectives.toml` is the built-in catalogue;
- the pixel sizes, fields of view and resolutions above, unless `--catalogue` is given;
- that fitting and the lights refuse what they should, and keep their state.

```sh
cargo run -p ex-objectives -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
