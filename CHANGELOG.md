# Changelog

All notable changes to glitchRx. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): before 1.0, a new feature raises
the middle number and a fix the last. Recipes and projects made by any
earlier version always open in a later one.

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

[0.2.0]: https://github.com/USER/glitchrx/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/USER/glitchrx/releases/tag/v0.1.0
