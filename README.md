# eonsort

[![Build](https://github.com/it-atelier-gn/eonsort/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/eonsort/actions)
[![Rust](https://img.shields.io/badge/rust-1.87%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Sorts files into date-based folders by the date they were actually created, not the date a backup
tool last touched them.

Point it at your photo folders, press **Scan**, and you get the full resulting tree as a preview
before a byte is copied. Then press **Copy files**. Scanning and copying only ever read your
sources; the one thing that touches them is the **Remove extra identical copies** button, which
sends duplicates to the recycle bin, and only when you press it.

Windows, Linux and macOS. Desktop app and `eonsort` command line tool.

![Eonsort desktop app](docs/screenshot.png)

Independent date sources, cross-checked, with wrong dates flagged and correctable by hand.
**[What it does, in full →](https://it-atelier-gn.github.io/eonsort/)**

---

## Install

Grab an installer from the [releases page](https://github.com/it-atelier-gn/eonsort/releases), or
build it yourself.

**Prerequisites**

- [Rust](https://rustup.rs/) 1.87+
- [Node.js](https://nodejs.org/) 20+ with npm
- [CMake](https://cmake.org/) and [NASM](https://nasm.us/), for the bundled libjpeg-turbo
- Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
- Windows: the [WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/), pre-installed on Windows 11 and most Windows 10

```sh
git clone https://github.com/it-atelier-gn/eonsort.git
cd eonsort
npm install
npx tauri dev          # npx tauri build for release bundles
```

Releases carry every optional feature. A build of your own turns them on by name:

```sh
cargo tauri build --features upright,tagging,quality,faces
```

| Feature   | What it adds                                     | Model                        |
| --------- | ------------------------------------------------ | ---------------------------- |
| `upright` | Turns sideways pictures, losslessly where it can | fetched from the setup panel |
| `tagging` | Reads what is in a picture, and searches by it   | fetched from the setup panel |
| `quality` | Rates how good a picture looks                   | fetched from the setup panel |
| `faces`   | Marks faces and lets you name them               | installed beside the program |

---

## Command line

The same engine ships as `eonsort`, for scheduled jobs and very large archives.

```sh
cargo build --release -p eonsort-cli
```

Sort in one step:

```sh
eonsort sort --source "D:\Photos" "D:\Downloads" --destination "E:\Sorted"
```

Or run the phases separately and review the plan in between:

```sh
eonsort scan --source "D:\Photos" --plan photos.jsonl
eonsort show --plan photos.jsonl --suspect     # only entries whose date looks wrong
eonsort offsets --plan photos.jsonl            # runs of files sharing one wrong clock
eonsort copy --plan photos.jsonl --destination "E:\Sorted"
eonsort verify --plan photos.jsonl --hash
eonsort undo --plan photos.jsonl               # take the copies back, sources untouched
eonsort places --into "D:\eonsort-data"        # fetch the place names, once
eonsort watch --source "D:\Incoming" --destination "E:\Sorted"
```

`undo` removes what a copy wrote and nothing else: a file that was already at the destination, or
edited since it landed, stays where it stands.

| Flag                                      | Meaning                                                                                                            |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `--pattern "%Y/%m"`                       | Destination layout. Any strftime pattern, plus `{token}` fields, e.g. `%Y/{city}`.                                 |
| `--name-pattern "%Y%m%d-{original_stem}"` | Layout of each copied file name. Left out, the name the file arrived with is kept.                                 |
| `--preset day`                            | Take both patterns from a ready-made layout. `eonsort presets` lists them.                                         |
| `--gazetteer "D:\data\geonames"`          | Folder holding the place names, as written by `eonsort places`.                                                    |
| `--destination "E:\Sorted"`               | Where the sorted tree goes. Optional on `scan`; on `copy` it sets or changes where an existing plan lands.         |
| `--provider filename exif`                | Only these date sources, in this order.                                                                            |
| `--strategy oldest`                       | Earliest date reported. `priority` stops at the first source that reports one.                                     |
| `--weight exif=45`                        | How much a source counts under the default strategy, 0 to 100. Repeat for more sources.                            |
| `--suspect`                               | On `show`, list only entries whose date looks wrong.                                                               |
| `--auto-rotate`                           | On `scan`, note sideways pictures so the copy turns them upright.                                                  |
| `--stamp-date`                            | On `copy`, write the chosen date into the EXIF block of each copy.                                                 |
| `--sidecars`                              | On `copy` and `sort`, leave an XMP sidecar beside each copy.                                                       |
| `--split-companions`                      | On `scan`, date each file on its own rather than keeping live photo, RAW and sidecar groups together.              |
| `--jobs 8`                                | Copy this many files in parallel. Left out, eonsort picks a number from the size of the files it is about to copy. |
| `--hash`                                  | Compare contents during `verify`, not just sizes.                                                                  |
| `--dry-run`                               | On `undo`, report what would be removed without removing it.                                                       |
| `--interval 60` `--settle 15`             | On `watch`, seconds between looks, and how long a file must stop growing before it is taken.                       |

Ctrl-C at any time; run the same command again to continue.

---

## Patterns

`--pattern` shapes the folders, `--name-pattern` the file names. Both take strftime fields with
`{token}` fields mixed in. Separate several tokens with `|` and the first one the file can answer
wins; end the list with a quoted word to always have an answer.

```sh
eonsort sort --source "D:\Photos" --destination "E:\Sorted" \
  --pattern '%Y/%Y-%m-%d/{city|region|country|"unknown place"}' \
  --name-pattern '%Y%m%d-%H%M%S-{original_stem}'
```

| Token                                            | What it is                                                             |
| ------------------------------------------------ | ---------------------------------------------------------------------- |
| `{subject}`                                      | The person the picture is filed under, when the pattern sorts by face. |
| `{city}` `{region}` `{country}` `{country_code}` | Where the picture was taken.                                           |
| `{camera_make}` `{camera_model}`                 | The camera that took it.                                               |
| `{original_name}` `{original_stem}` `{ext}`      | The name the file arrived with, with and without its extension.        |

The place tokens come from the picture's own GPS reading, matched against a list of places kept on
this machine — nothing leaves the computer. Fetch the list once with `eonsort places`, or from the
desktop app's setup panel, about 11 MB.

`eonsort presets` lists the ready-made layouts: `plain`, `day`, `place`, and one each for `immich`,
`photoprism` and `elodie`.

---

## Sample pictures

`npm run samples` fetches pictures to try eonsort on. Every one is CC0 and carries a date of its
own, spread across contributors and years, and some carry a GPS reading as well.
`samples/manifest.json` records where each came from.

```sh
npm run samples -- samples 50
eonsort sort --source samples --destination sorted --pattern '%Y/{city|country|"unknown place"}'
```

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first.

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run check && npm test
```

The optional features are not in that line; CI checks them in separate non-blocking jobs. If you
touch `upright.rs`, `yolo.rs`, `tagging.rs`, `quality.rs`, `yunet.rs`, `sface.rs` or `faces.rs`, run
clippy and the tests with the matching feature as well.

The application icon is generated. To change it, edit `scripts/make-icon.mjs` and run `npm run icon`.

### End-to-end UI tests

`tests/e2e` drives the actual desktop window through [WebdriverIO](https://webdriver.io/) and
[`tauri-driver`](https://crates.io/crates/tauri-driver). Windows and Linux only, because
`tauri-driver` has no macOS WebDriver backend.

```sh
cargo install tauri-driver --locked
npm run test:e2e
```

On Windows the matching Edge WebDriver downloads automatically. On Linux, install
`webkit2gtk-driver` (or your distro's equivalent) so `WebKitWebDriver` is on `PATH`.

**Close any running eonsort window first.** The rebuild cannot replace a locked `.exe`, and the
suite will then quietly run the previous binary and pass without testing your changes.

---

## License

MIT © 2026 Georg Nelles

---

## Credits

Place names come from [GeoNames](https://www.geonames.org/), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), downloaded on request.

Face detection uses [YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet)
by Shiqi Yu, MIT licensed. Telling one face from another uses
[SFace](https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface) by Zhong
Yaoyao and Deng Weihong, Apache 2.0. Both sit in `models/` beside the executable with their licences
as `models/yunet-LICENSE` and `models/sface-LICENSE`, and are checked against the detector and the
recogniser OpenCV ships by tests that run on every build. `scripts/yunet-to-safetensors.py`,
`scripts/sface-to-safetensors.py`, `scripts/yunet-reference-faces.py` and
`scripts/sface-reference-feature.py` convert fresh copies and regenerate those fixtures; all need
the original `.onnx` and run under `uv`.

Tagging uses [SigLIP](https://huggingface.co/google/siglip-base-patch16-224) by Google, Apache 2.0.
Rating uses [aesthetics-predictor-v1](https://huggingface.co/shunk031/aesthetics-predictor-v1-vit-base-patch32)
by Shunsuke Kitada, which puts the [LAION aesthetic predictor v1](https://github.com/LAION-AI/aesthetic-predictor)
over [OpenAI CLIP](https://github.com/openai/CLIP), both MIT licensed. Both are downloaded on
request, pinned to a revision, and named in the setup panel beside the switch that turns them on.
