# eonsort

[![Build](https://github.com/it-atelier-gn/eonsort/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/eonsort/actions)
[![Rust](https://img.shields.io/badge/rust-1.87%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Sorts files into date-based folders using the date they were actually created, not the date a backup tool last touched them.

Point it at your photo folders, press **Scan**, and you get a full preview of the resulting tree before a single byte is copied. Browse the planned folders, click any file to see it, then press **Copy files**. Sources are never modified.

Runs on Windows, Linux and macOS, as a desktop app and as an `eonsort` command line tool.

---

## How the date is found

Four sources are consulted, and by default the **earliest** date any of them reports wins:

| Source | What it reads |
|---|---|
| **File name** | `IMG_20230506_101112.jpg`, `Screenshot 2021-07-04 at 09.05.01.png`, `Rechnung 06.05.2021.pdf`, and many more shapes |
| **EXIF** | `DateTimeOriginal` and friends in JPEG, TIFF, PNG, WebP, HEIF and common raw formats |
| **Media** | The recording time in the `mvhd` box of MP4, MOV, M4A and related containers |
| **File system** | Whichever of the created / modified timestamps is older |

Each source can be switched off, and you can swap the *oldest wins* rule for *first match wins*, which walks the list in order and stops at the first hit.

## Safety

- **Nothing is overwritten.** If a file of the same name is already in the target folder, contents are compared. Identical files are left alone; different ones are stored beside it as `name_dup_1.jpg`.
- **Every copy is atomic.** Files are written to a staging folder, flushed to disk, then renamed into place. An interrupted run never leaves a half-written file under a real name.
- **Everything resumes.** The scan appends to its plan as it goes and the copy keeps a journal, so stopping a run — or losing power — costs you nothing. Start the same run again and it continues where it stopped.
- **Sources are only ever read.**

## Quick Start

Grab an installer from the [releases page](https://github.com/it-atelier-gn/eonsort/releases), or build it yourself.

### Prerequisites

- [Rust](https://rustup.rs/) 1.87+
- [Node.js](https://nodejs.org/) 20+ with npm
- Linux only: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
- Windows only: the [WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/), pre-installed on Windows 11 and most Windows 10 systems

### Build & Run

```sh
git clone https://github.com/it-atelier-gn/eonsort.git
cd eonsort
npm install
npx tauri dev
```

To build release bundles for your platform:

```sh
npx tauri build
```

---

## Command line

The same engine ships as `eonsort`, which is handy for scheduled jobs and very large archives.

```sh
cargo build --release -p eonsort-cli
```

Sort in one step:

```sh
eonsort sort --source "D:\Photos" "D:\Downloads" --destination "E:\Sorted"
```

Or run the phases separately and review the plan in between:

```sh
eonsort scan --source "D:\Photos" --destination "E:\Sorted" --plan photos.jsonl
```

```sh
eonsort show --plan photos.jsonl
```

```sh
eonsort copy --plan photos.jsonl
```

```sh
eonsort verify --plan photos.jsonl --hash
```

Useful flags:

| Flag | Meaning |
|---|---|
| `--pattern "%Y/%m"` | Destination layout. Any strftime pattern works, for example `%Y/%Y-%m-%d`. |
| `--provider filename exif` | Only consult these date sources, in this order. |
| `--strategy priority` | Stop at the first source that reports a date instead of taking the oldest. |
| `--jobs 8` | Copy this many files in parallel. |
| `--hash` | Compare file contents during `verify`, not just sizes. |

Press Ctrl-C at any time. Run the same command again to continue.

---

## How it is put together

```
crates/eonsort-core   the engine: date providers, scanning, planning, copying, verifying
crates/eonsort-cli    the eonsort command line tool
src-tauri             the desktop app backend and its Tauri commands
src                   the SvelteKit front end
```

A **plan** is a JSON Lines file: one header record, then one record per file describing where it came from, which source supplied its date, and where it will go. The copy step reads the plan and writes its own journal next to it. Both files are append-only, which is what makes every phase restartable.

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first to discuss the approach.

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run check && npm test
```

The application icon is generated rather than hand-drawn. To change it, edit `scripts/make-icon.mjs` and run `npm run icon`.

---

## License

MIT © 2026 Georg Nelles
