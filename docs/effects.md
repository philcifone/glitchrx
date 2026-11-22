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

## echo

Feedback delay: every sample adds a fading copy of the output `delay` earlier.

`y[n] = x[n] + decay * y[n - delay]`. A delay of a fraction of a row ghosts the picture sideways, whole rows ghost it downwards. Repeats stack until they fade, and the sum is clipped, which is where the bright bands come from.

Follows Audacity's Echo. Version: 1.

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

