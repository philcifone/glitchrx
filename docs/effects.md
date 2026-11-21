# Effect reference

Generated from the effect registry (`crates/glitchrx-core/src/effects`). Do not edit by hand; run `UPDATE_DOCS=1 cargo test -p glitchrx-core --test docs` to refresh.

All byte effects edit the de-stuffed scan data (the payload) in windows called hits. Arithmetic wraps modulo 256, so values never clip, and that is what makes most effects exactly reversible. Percent positions and lengths are relative to the payload size, so defaults look similar across image resolutions.

Two things to expect. First, payload position does not map evenly onto image area: detailed regions (foliage, noise, text) use far more bytes than smooth ones (sky), so hits mostly land in the busy parts of a picture. Second, damage is not local: a hit usually affects everything after it in the image, as colour drift (DC drift) and smeared blocks (Huffman desync). See VOCAB.md for both terms.

## echo

Feed-forward echo: each byte gets quieter copies of the bytes `delay`, 2x`delay`, ... before it added on.

`y[i] = x[i] + sum over taps k of (q_k * x[i - k*delay]) >> 8`, modulo 256, where `q_k` is `gain^k` in Q8 fixed point. Because the added terms depend only on the input, the inverse subtracts them again walking forward through the window.

Reversible: yes. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `delay` | `1024` | distance in bytes between a byte and its echo |
| `gain` | `0.5` | loudness of the first echo, 0..1 |
| `taps` | `1` | number of echoes; tap k is gain^k loud at k x delay |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `3` | number of glitch windows; 0 = one window over the whole region |
| `len` | `0.5%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --echo delay=2048,gain=0.6,taps=2 photo.jpg
```

## reverb

Comb-filter reverb: three feedback echoes in series, with delays `delay`, 1.37x and 1.71x `delay`.

Each comb computes `y[i] = x[i] + (q * y[i - d]) >> 8` modulo 256, feeding its own output back, so echoes repeat and decay. A true audio reverb is far more complex; this is a deliberately simple approximation. Interesting property: the inverse, `x[i] = y[i] - (q * y[i - d]) >> 8`, only needs output values, so it is exact even though the forward pass is recursive.

Reversible: yes. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `delay` | `512` | delay of the first comb in bytes; the others are 1.37x and 1.71x |
| `feedback` | `0.4` | how much of each comb's output is fed back, 0..1 |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `2` | number of glitch windows; 0 = one window over the whole region |
| `len` | `1%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --reverb delay=800,feedback=0.6 photo.jpg
```

## shift

Rotate each window by `by` bytes; bytes pushed off the end wrap to the start.

Moves a stretch of compressed data sideways. Typically shows up as blocks displaced horizontally and colour shifts from the point of the hit onwards.

Reversible: yes. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `by` | `37` | bytes to rotate by; negative rotates the other way |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `4` | number of glitch windows; 0 = one window over the whole region |
| `len` | `1%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --shift by=200,hits=6 photo.jpg
```

## backwards

Play each window backwards: reverse its byte order.

The audio equivalent of reversing a clip. Reversing twice restores the original, so the effect is its own inverse.

Reversible: yes. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `3` | number of glitch windows; 0 = one window over the whole region |
| `len` | `0.2%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --backwards len=1k,hits=8 photo.jpg
```

## shuffle

Cut each window into blocks of `block` bytes (or percent of the window) and swap a `fraction` of them around.

Blocks are chosen and permuted with the seeded RNG, so the inverse simply regenerates the same permutation and moves every block back. A percent block size scales with the window, so the default behaves alike on small and large images. Any partial block at the end of a window is left in place.

Reversible: yes. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `block` | `5%` | block size: bytes, or percent of the window |
| `fraction` | `0.5` | share of blocks that move, 0..1 |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `2` | number of glitch windows; 0 = one window over the whole region |
| `len` | `2%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --shuffle block=1k,fraction=0.8 photo.jpg
```

## amplify

Scale bytes away from the midpoint 128 by `factor`.

`wrap` mode computes `(x - 128) * factor + 128` modulo 256 with an odd whole-number factor. Odd numbers have a multiplicative inverse modulo 256 (3 * 171 = 513 = 2*256 + 1), so multiplying by that inverse undoes the effect exactly. `clip` mode is the Audacity-style version: any decimal factor, values clamped to 0..255, information lost.

Reversible: depends: wrap mode yes, clip mode no. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `factor` | `3` | gain; wrap mode needs an odd whole number |
| `mode` | `wrap` | wrap (reversible) or clip (lossy, Audacity-like) |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `3` | number of glitch windows; 0 = one window over the whole region |
| `len` | `0.3%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --amplify factor=5 photo.jpg
```

## bitcrush

Keep only the top `bits` bits of every byte; the rest become zero.

The audio bit-crusher reduces sample resolution. On compressed data it collapses many different codes onto the same values. The discarded bits are gone, so the effect cannot be reversed.

Reversible: no. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `bits` | `4` | bits kept per byte (1 = harshest) |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `3` | number of glitch windows; 0 = one window over the whole region |
| `len` | `0.3%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --bitcrush bits=2,hits=5 photo.jpg
```

## stutter

Copy the start of each window (`chunk` bytes, or percent of the window) and repeat it until the window is full.

Like a skipping CD. The bytes that get overwritten are lost, so the effect cannot be reversed. The file length never changes: the repeats replace data rather than insert it.

Reversible: no. Version: 1.

| Parameter | Default | Meaning |
| --- | --- | --- |
| `chunk` | `12%` | length of the repeated chunk: bytes, or percent of the window |
| `region` | `5%:100%` | where hits may land, START:END in bytes or percent of the image data |
| `hits` | `3` | number of glitch windows; 0 = one window over the whole region |
| `len` | `0.5%` | length of each window, bytes (4k, 1m) or percent of the image data |

Example:

```
glitchrx --stutter chunk=64,len=8k photo.jpg
```

