<p align="center"><img src="assets/logo.svg" alt="modesty-gui" width="110" height="110"></p>

# modesty-gui

> Desktop GUI (Tauri)

**Part of the [Modesty toolkit](https://github.com/sniffis-commits/modesty-meta)** — see the
[big README](https://github.com/sniffis-commits/modesty-meta) for the full picture, the rating scale and all repositories.

## What this is

A cross-platform desktop application around the scanning library: visual dashboard, media scan settings, hit-frame review, floating rating overlay and `.skp` skip-list generation. Scanning stays local unless you enable the optional hosted scan.

## How to


```bash
npm install
npm run tauri dev          # development
npm run tauri build        # release binary
```
- Configure the model, fps/every-frame, subtitles, decoder and threshold in the UI.
- Hosted scan is off by default; enabling it uploads the selected media.
- Requires the Rust toolchain and Tauri system dependencies.


## Layout

| Path | Purpose |
|---|---|
| `src/` | source from the monorepo |
| `src-tauri/` | source from the monorepo |

## More

- Full overview and other repos: [modesty-meta](https://github.com/sniffis-commits/modesty-meta)
- Website: [https://modesty.cia.org.il](https://modesty.cia.org.il)
- Model weights: [modesty-models](https://github.com/sniffis-commits/modesty-models)
- Licence: MIT (see `LICENSE`). Third-party data/model terms are on the
  [site's licence page](https://modesty.cia.org.il/?p=licenses).
