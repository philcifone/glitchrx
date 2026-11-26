# Changelog

All notable changes to glitchRx. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): before 1.0, a new feature raises
the middle number and a fix the last. Recipes and projects made by any
earlier version always open in a later one.

## [Unreleased]

### Added

- **Filters**, a new group in the palette (D-71): `lowpass`, `highpass`,
  `bandpass`, `notch` and `basstreble`, after Audacity's Low-Pass Filter,
  High-Pass Filter, Notch Filter and Bass and Treble. The cutoff is a size
  (`size=8px`), not a frequency, since a picture has no sample rate.

### Changed

- Effect settings with a few options show them all as buttons instead of
  a dropdown, and sizes in px get a slider.
- A linear gradient mask stays level more easily: only its dashed end
  turns it (settling on level and plumb), the solid end slides along it,
  and tapping both ends selects them so a drag moves the whole gradient.

## [0.2.1] - 2026-11-25

### Fixed

- A zoomed photo can be panned a little past its edges (a quarter of the
  screen, up to 160 px), so its border can be masked or pulled away from
  the edge of the screen. On a phone, the step panel's rounded top used
  to hide the bottom 14 px for good.

## [0.2.0] - 2026-11-25

### Added

- **Crop the result.** The crop editor has *Crop the: Photo | Result*.
  Result glitches the whole photo and then frames it, so the glitch inside
  the frame stays as it is; a new crop starts on Result. CLI:
  `--crop-result`. Recipes record it as `crop_result = true` (recipe
  version 8).
- **Save As updates a project.** Saving a project onto the name of an
  existing project asks "Update NAME.glrx?" and replaces it. Photos and
  other files are still never overwritten.
- **Collapse all and an Edit button** beside Steps: double carets fold every
  card shut or opens them all, and a pencil opens Edit mode. Holding a step
  still works.
- **A note on work sizes**, behind an info button in the photo's toolbox.

### Changed

- The encoding and the photo's tools sit side by side in two cards, with
  their notes in one line underneath.
- Cropping takes the whole screen, and its button reads Crop.
- Open has two equal buttons, From Device and Open .glrx.
- Opening another photo drops the crop.
- In Edit mode the held step starts out selected, and selected rows show
  a green ring.
- Recipes and projects are written as the oldest version that holds them,
  so files that use nothing new still open on 0.1.0.

## [0.1.0] - 2026-11-24

First release: the engine (14 audio and data effects, pixel sort, pull,
four adjustments), recipes, projects (`.glrx`), the web GUI, the desktop
app and the command line.

[0.2.1]: https://github.com/USER/glitchrx/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/USER/glitchrx/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/USER/glitchrx/releases/tag/v0.1.0
