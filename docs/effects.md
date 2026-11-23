# Effect reference

Generated from the effect registry (`crates/glitchrx-core/src/effects`). Do not edit by hand; run `UPDATE_DOCS=1 cargo test -p glitchrx-core --test docs` to refresh.

Every effect works on the decoded image as raw bytes: R, G, B for each pixel, row after row, the way an uncompressed TIFF stores it. Each step reads those bytes as audio samples in an *encoding* (as Audacity's Import Raw Data does), applies the effect, and clips the result back to bytes.

Sizes take units: `%` of the image (which runs top to bottom), `rows` (a fractional row is that share of the width), `px`, or plain bytes. A distance of whole pixels keeps colours; `phase` adds 1 or 2 bytes, which rotates red into green into blue.

## Encodings

| Encoding | Bytes | Character |
| --- | --- | --- |
| `u8` | 1 | unsigned 8-bit: clean ghosts, colours rotate |
| `s8` | 1 | signed 8-bit: clips at mid-brightness, inverts highlights |
| `s16le` | 2 | signed 16-bit: ghosts under rainbow grain |
| `s16be` | 2 | signed 16-bit: ghosts under rainbow grain |
| `s24le` | 3 | signed 24-bit: one sample per pixel |
| `s24be` | 3 | signed 24-bit: one sample per pixel |
| `ulaw` | 1 | companded 8-bit: posterized neon blocks |
| `alaw` | 1 | companded 8-bit: posterized neon blocks |

Set once per recipe (`--encoding`, `--channels`), or per step with `encoding=` and `channels=` in the step's parameters.

## Every step

- `opacity` (0 to 1, default 1): blend the step's result over the picture it received, as with a layer's opacity. Example: `--echo opacity=0.5`.
- A mask (recipe files and the GUI): where the result shows, made of brushes, gradients, circles and brightness ranges. See `[step.mask]` in SPEC section 8.

## echo

Feedback delay: every sample adds a fading copy of the output `delay` earlier.

`y[n] = x[n] + decay * y[n - delay]`. A delay of a fraction of a row ghosts the picture sideways, whole rows ghost it downwards. Repeats stack until they fade, and the sum is clipped, which is where the bright bands come from.

Follows Audacity's Echo. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `delay` | `0.33rows` | distance to the repeat (rows, px, % or bytes) |
| `phase` | `1` | extra bytes on top of the distance: 0 keeps colours, 1 or 2 rotates them |
| `decay` | `0.5` | loudness of each repeat relative to the one before |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --echo delay=0.33rows,decay=0.5 photo.jpg
```

## delay

A fixed number of repeats of the input, each `decay` times quieter than the last.

`y[n] = x[n] + decay * x[n - d] + decay^2 * x[n - 2d] + ...` for `taps` repeats. Unlike `echo`, repeats come from the input, not the output, so they stop after `taps` copies: crisper, countable ghosts.

Follows Audacity's Delay. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `delay` | `0.33rows` | distance between repeats (rows, px, % or bytes) |
| `phase` | `1` | extra bytes on top of the distance: 0 keeps colours, 1 or 2 rotates them |
| `decay` | `0.5` | loudness of each repeat relative to the one before |
| `taps` | `3` | number of repeats |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --delay delay=2rows,taps=4 photo.jpg
```

## reverb

Freeverb: eight damped feedback combs in parallel, then four allpass diffusers.

Smears the picture into a haze of overlapping ghosts. `size` is the longest comb delay; the others keep Freeverb's ratios to it, so the ghosts never line up. `room` sets how long the tail rings, `damping` how fast fine detail dies out of it.

Follows Audacity's Reverb. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `size` | `0.5rows` | longest comb delay (rows, px, % or bytes) |
| `phase` | `1` | extra bytes on top of the distance: 0 keeps colours, 1 or 2 rotates them |
| `room` | `0.7` | length of the tail |
| `damping` | `0.5` | how fast detail fades from the tail |
| `wet` | `0.33` | level of the reverb |
| `dry` | `0.5` | level of the original (0.5 = unchanged) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --reverb room=0.9,wet=0.5 photo.jpg
```

## amplify

Multiply every sample by a gain given in decibels, clipping at the limits.

Positive gains push bytes away from the encoding's silence point and clip them: with `u8` that is a harsh contrast boost around mid grey, with `s8` it folds highlights into shadows, with the multi-byte encodings it explodes into noise.

Follows Audacity's Amplify. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `gain` | `6` | gain in dB (+6 dB doubles, -6 dB halves) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `4` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `5%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --amplify gain=12,hits=6 photo.jpg
```

## invert

Flip every sample upside down.

With `u8` this is close to a photographic negative. With `s8` and the multi-byte encodings the flip happens around a different point in each byte, so only some channels or some brightness ranges invert.

Follows Audacity's Invert. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `4` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `4%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --invert encoding=s16le,hits=8 photo.jpg
```

## bitcrush

Reduce every sample to `bits` bits of resolution.

Posterizes. With `u8` it is plain colour banding; with the multi-byte encodings the lower bytes of each sample are wiped, so whole channels drop to flat values while the others keep their detail.

Follows Audacity's a bitcrusher (Audacity has none built in; the usual Nyquist plug-ins). Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `bits` | `3` | bits kept per sample |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --bitcrush bits=2,encoding=s16be photo.jpg
```

## backwards

Play each window backwards.

Reverses the order of the samples (not the bytes inside a sample). A window covering whole rows mirrors them left to right and flips their order; with `u8` the colour channels also swap, red with blue.

Follows Audacity's Reverse. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `6` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `2%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --backwards hits=10,len=3rows photo.jpg
```

## shift

Rotate each window forward by `by`; what falls off the end wraps to the start.

A fraction of a row slides a band of the picture sideways. `phase` adds bytes to the distance, which moves the colour channels out of step (red into green, green into blue). Edits move whole samples, so with multi-byte encodings the distance is rounded down to a whole sample.

Follows Audacity's cut and paste. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `by` | `0.25rows` | distance (rows, px, % or bytes) |
| `phase` | `1` | extra bytes on top of the distance: 0 keeps colours, 1 or 2 rotates them |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `6` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `3%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --shift by=0.5rows,phase=2,hits=12 photo.jpg
```

## shuffle

Split each window into blocks and swap a share of them at random.

Blocks of whole rows rearrange bands of the picture; blocks of part of a row tear it into sideways-displaced strips. A percentage `block` is of the window, not the image. Seeded, so the same seed picks the same blocks.

Follows Audacity's cut and paste. Version: 1.

The seed picks where hits land and what happens inside each window.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `block` | `0.5rows` | block size (rows, px, bytes, or % of the window) |
| `fraction` | `0.5` | share of the blocks that move |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `4` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --shuffle block=2rows,fraction=1 photo.jpg
```

## stutter

Copy the first `chunk` of each window over the rest of it, again and again.

Whole rows repeat a strip of the picture down the window; part of a row gives a diagonal smear, since each repeat starts a little further along the row. A percentage `chunk` is of the window.

Follows Audacity's Repeat. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `chunk` | `1rows` | length of the repeated piece (rows, px, bytes, or % of the window) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `3` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `5%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --stutter chunk=0.9rows,hits=5 photo.jpg
```

## phaser

All-pass stages swept by a slow oscillator, mixed with the input: wavy, combed bands.

Audacity's phaser: `stages` first-order all-pass filters in series, their coefficient swept between 1 and `1 - depth` by an LFO, with `feedback` from the output back into the input, then mixed with the dry signal. In a picture the sweep runs down the rows, so one LFO `period` is one wave of the banding. Audacity updates the sweep every 20 samples, here every 16.

Follows Audacity's Phaser. Version: 1.

Live previews of this effect are approximate; the GUI renders it at full resolution when you pause.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `stages` | `4` | all-pass stages; more gives more, finer notches |
| `period` | `6rows` | length of one LFO sweep (rows, px, % or bytes) |
| `start` | `0` | where in its sweep the LFO starts, in turns (0.5 is half way) |
| `depth` | `0.4` | how far the sweep reaches |
| `feedback` | `0.5` | output fed back into the input; negative inverts it |
| `mix` | `0.5` | share of the phased signal in the output (Audacity's dry/wet) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --phaser stages=8,period=3rows,feedback=0.8 photo.jpg
```

## wahwah

A resonant filter whose cutoff an LFO sweeps up and down: smears that pulse down the picture.

Audacity's wahwah: a two-pole resonant low-pass (a biquad) whose cutoff follows an LFO, from `offset` up to `offset + depth` of the range on an exponential scale. Low cutoffs smear each row sideways; high `resonance` rings, which clips into bright fringes. One LFO `period` is one pulse down the rows. Audacity updates the filter every 30 samples, here every 16.

Follows Audacity's Wahwah. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `period` | `4rows` | length of one LFO sweep (rows, px, % or bytes) |
| `start` | `0` | where in its sweep the LFO starts, in turns (0.5 is half way) |
| `depth` | `0.7` | how far the cutoff sweeps |
| `resonance` | `2.5` | peak at the cutoff; high values ring |
| `offset` | `0.3` | lowest cutoff, as a share of the range |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --wahwah period=8rows,resonance=6 photo.jpg
```

## distort

Drive the samples into a curve: hard clipping, soft saturation, or folding back.

Each sample is multiplied by `drive` (dB) and bent by a curve. `hard` clips at full scale, like amplify. `soft` rounds off towards full scale (a cubic curve), for posterized but smooth tones. `fold` reflects anything past full scale back in, again and again, so bright areas turn into stacked bands of inverted contour. `mix` blends with the input.

Follows Audacity's Distortion. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `curve` | `fold` | shape of the distortion |
| `drive` | `12` | gain into the curve, in dB |
| `mix` | `1` | share of the distorted signal in the output |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --distort curve=fold,drive=18 photo.jpg
```

## speed

Play each window faster or slower, wrapping around to keep its length: shears and stretches.

Output sample `i` reads the input at `i * factor`, between samples by linear interpolation, wrapping back to the start of the window. Faster than 1 squeezes the window's rows and repeats it, and since a squeezed row no longer lines up with the picture's rows, everything leans into a diagonal shear. Slower than 1 stretches the start of the window across all of it. With `step=pixel` (the default) whole pixels are resampled, so colours hold. `step=sample` resamples each sample like Audacity, which drifts across the R, G and B bytes and blends them into grey stripes.

Follows Audacity's Change Speed. Version: 1.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `factor` | `1.5` | playback speed; above 1 squeezes and repeats, below 1 stretches |
| `step` | `pixel` | resample whole pixels (colours hold) or single samples (grey stripes) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `4` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `8%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --speed factor=1.1,hits=6 photo.jpg
```

## pixelsort

Sort runs of pixels by brightness, hue or saturation: streaks that melt down or across the picture.

Each column (or row, with `direction=horizontal`) is cut into runs of pixels whose brightness lies between `low` and `high`; pixels outside that band stay put and end the runs. Every run is then sorted by `key`: `descending` puts the highest value first (at the top, or on the left), `ascending` the lowest. Raise `low` to let more shadows hold the picture together; lower it to melt more. `max` caps the length of a run (`%` is of the column or row, so `100%` is no cap), which cuts long smooth areas into repeated streaks.

Version: 1.

A pixel effect: it works on whole RGB pixels, so the encoding and channels do not apply.

Live previews of this effect are approximate; the GUI renders it at full resolution when you pause.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `direction` | `vertical` | sort down the columns or along the rows |
| `key` | `brightness` | what the pixels in a run are sorted by |
| `order` | `descending` | descending puts the highest first (top or left) |
| `low` | `0.25` | darkest brightness that is sorted; darker pixels stay and break runs |
| `high` | `0.9` | brightest brightness that is sorted; brighter pixels stay and break runs |
| `max` | `100%` | longest run in px or % of the column or row (100% = no limit) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --pixelsort low=0.3,region=25%:100% photo.jpg
```

## levels

Set the black point, white point and midtone gamma.

Input values at or below `black` become `out_black`, at or above `white` become `out_white`, and the range between is stretched to fit. `gamma` bends the middle: above 1 lightens the midtones, below 1 darkens them. `channel` limits it to one colour, which shifts the colour balance.

Version: 1.

An adjustment: a photo tool rather than a glitch. It works on whole RGB pixels, so the encoding and channels do not apply; masks, opacity and placement do, as for any step.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `channel` | `rgb` | which colours it changes: all three, or one |
| `black` | `12` | input black point: this value and below become out_black |
| `white` | `240` | input white point: this value and above become out_white |
| `gamma` | `1` | midtones: above 1 lighter, below 1 darker |
| `out_black` | `0` | darkest output value |
| `out_white` | `255` | lightest output value (below out_black inverts) |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --levels black=20,white=230,gamma=1.2 photo.jpg
```

## curves

Reshape the tones with a curve through a few points.

`points` lists `IN:OUT` pairs from 0 to 255, inputs rising, joined by `/`. The default is a gentle S, which adds contrast; `0:0/255:255` changes nothing, and `0:255/255:0` is a negative. The curve passes smoothly through every point without overshooting. `channel` limits it to one colour.

Version: 1.

An adjustment: a photo tool rather than a glitch. It works on whole RGB pixels, so the encoding and channels do not apply; masks, opacity and placement do, as for any step.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `channel` | `rgb` | which colours it changes: all three, or one |
| `points` | `0:0/64:52/192:204/255:255` | IN:OUT pairs, 0 to 255, inputs rising, joined by / |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --curves points=0:20/128:150/255:235,channel=blue photo.jpg
```

## hsl

Turn the hues, and change saturation and lightness.

`range` limits it to one family of colours (`reds`, `yellows`, ...), fading softly into the neighbouring ones; greys are left alone. `hue` turns every colour round the colour wheel, in degrees (120 makes red green, green blue and blue red). `saturation` from -100 (grey) to 100 (twice as vivid, clipping at full). `lightness` from -100 (black) to 100 (white). Greys stay grey under hue and saturation.

Version: 1.

An adjustment: a photo tool rather than a glitch. It works on whole RGB pixels, so the encoding and channels do not apply; masks, opacity and placement do, as for any step.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `range` | `all` | which colours change: all, or one family |
| `hue` | `0` | turn the colours round the wheel, in degrees |
| `saturation` | `30` | -100 grey, 0 unchanged, 100 twice as vivid |
| `lightness` | `0` | -100 black, 0 unchanged, 100 white |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --hsl hue=40,saturation=-50 photo.jpg
```

## balance

Tint the shadows, midtones or highlights.

Pick the `tones`, then push them towards red or cyan, green or magenta, blue or yellow (-100 to 100; at 100 a full-weight pixel moves by about 100 of 255). Add a step for each range you want to tint: warm highlights and cool shadows are two steps. `luminosity=keep` restores each pixel's brightness so only the colour changes.

Version: 1.

An adjustment: a photo tool rather than a glitch. It works on whole RGB pixels, so the encoding and channels do not apply; masks, opacity and placement do, as for any step.

The seed only picks where hits land, so it has no effect with `hits=0`.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `tones` | `midtones` | which tones are tinted |
| `red` | `12` | towards red; below 0 towards cyan |
| `green` | `0` | towards green; below 0 towards magenta |
| `blue` | `-12` | towards blue; below 0 towards yellow |
| `luminosity` | `keep` | keep each pixel's brightness, so only the colour changes |
| `region` | `0%:100%` | where hits may land, START:END; percent of the image runs top to bottom |
| `hits` | `0` | number of windows at seeded places; 0 = one window over the whole region |
| `len` | `10%` | length of each window (%, rows, px or bytes); unused when hits=0 |

Example:

```
glitchrx --balance tones=shadows,blue=30 --balance tones=highlights,red=20,blue=-20 photo.jpg
```

