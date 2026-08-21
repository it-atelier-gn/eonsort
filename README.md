# eonsort

[![Build](https://github.com/it-atelier-gn/eonsort/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/eonsort/actions)
[![Rust](https://img.shields.io/badge/rust-1.87%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Sorts files into date-based folders by the date they were actually created, not the date a backup
tool last touched them.

Point it at your photo folders, press **Scan**, and you get the full resulting tree as a preview
before a byte is copied. Then press **Copy files**. Sources are only ever read.

Windows, Linux and macOS — desktop app and `eonsort` command line tool.

![Eonsort desktop app](docs/screenshot.png)

Seven independent date sources, cross-checked; camera-clock resets and other wrong dates flagged and
correctable by hand; pictures turned upright, duplicates found, and — optionally — tagged and rated
by two models running locally on the CPU.
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

Releases ship with every optional feature. A build of your own turns them on by name:

```sh
cargo tauri build --features upright,tagging,quality
```

`upright` turns sideways pictures losslessly, `tagging` reads what is in them, `quality` rates them.
Both models are downloaded once from the setup panel into `models/` in the app data folder.

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
eonsort copy --plan photos.jsonl --destination "E:\Sorted"
eonsort verify --plan photos.jsonl --hash
```

| Flag | Meaning |
|---|---|
| `--pattern "%Y/%m"` | Destination layout. Any strftime pattern, e.g. `%Y/%Y-%m-%d`. |
| `--destination "E:\Sorted"` | Where the sorted tree goes. Optional on `scan`; on `copy` it sets or changes where an existing plan lands. |
| `--provider filename exif` | Only these date sources, in this order. |
| `--strategy oldest` | Earliest date reported. `priority` stops at the first source that reports one. |
| `--weight exif=45` | How much a source counts under the default strategy, 0 to 100. Repeat for more sources. |
| `--suspect` | On `show`, list only entries whose date looks wrong. |
| `--auto-rotate` | On `scan`, note sideways pictures so the copy turns them upright. |
| `--stamp-date` | On `copy`, write the chosen date into the EXIF block of each copy. |
| `--split-companions` | On `scan`, date each file on its own rather than keeping live photo, RAW and sidecar groups together. |
| `--jobs 8` | Copy this many files in parallel. Left out, eonsort picks a number from the size of the files it is about to copy. |
| `--hash` | Compare contents during `verify`, not just sizes. |

Ctrl-C at any time; run the same command again to continue.

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first.

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run check && npm test
```

The optional features are not in that line; CI checks them in separate non-blocking jobs. If you
touch `upright.rs`, `yolo.rs`, `tagging.rs` or `quality.rs`, run clippy and the tests with the
matching feature as well.

The application icon is generated. To change it, edit `scripts/make-icon.mjs` and run `npm run icon`.

### End-to-end UI tests

`tests/e2e` drives the actual desktop window through [WebdriverIO](https://webdriver.io/) and
[`tauri-driver`](https://crates.io/crates/tauri-driver). Windows and Linux only — `tauri-driver` has
no macOS WebDriver backend.

```sh
cargo install tauri-driver --locked
npm run test:e2e
```

On Windows the matching Edge WebDriver downloads automatically. On Linux, install
`webkit2gtk-driver` (or your distro's equivalent) so `WebKitWebDriver` is on `PATH`.

**Close any running eonsort window first** — the rebuild cannot replace a locked `.exe`, and the
suite will then quietly run the previous binary and pass without testing your changes.

---

## License

MIT © 2026 Georg Nelles
