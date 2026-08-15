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

---

## How the date is found

| Source | What it reads |
|---|---|
| **File name** | `IMG_20230506_101112.jpg`, `Screenshot 2021-07-04 at 09.05.01.png`, `Rechnung 06.05.2021.pdf`, and many more shapes |
| **EXIF** | `DateTimeOriginal` and friends in JPEG, TIFF, PNG, WebP, HEIF and common raw formats |
| **Media** | Recording time in the `mvhd` box of MP4, MOV, M4A and related containers |
| **File system** | Whichever of created / modified is older |

Every source that reports a date is kept. Each can be switched off. Three rules choose between them:

| Rule | What it does |
|---|---|
| **Weigh the evidence** (default) | Discards impossible dates, then prefers the one two sources agree on |
| **Oldest date wins** | Keeps the earliest date any source reports |
| **First match wins** | Walks the sources in order, stops at the first hit |

### The optional local model

A fifth source reads the date **printed into the picture** — a film camera's corner stamp, a date on
a scanned document. It needs a model runner you host, so nothing leaves your machine.

Point eonsort at [Ollama](https://ollama.com) or any OpenAI-compatible endpoint and press **Check
connection**. With Ollama, **Download** pulls a model with a progress bar and a **Stop** button, and
**Remove** deletes it behind a confirmation; both are Ollama-only, since an OpenAI-compatible
endpoint has no install API.

Reading pictures is off by default and costs about a second per image, so it runs only on files you
ask about unless you turn on "Look at every picture during the scan".

## When the date is wrong

A camera whose battery died resets its clock to a factory default — usually 1 January of 2000, 2003
or 2015 — and every photo after that carries a date that looks real. Eonsort flags rather than
quietly misfiles:

- a date on a factory-reset instant, in the future, or later than the file was written
- a run of files counting up from a reset date while the files were written years later
- a batch sharing one timestamp to the second
- a file stranded years from everything else in its folder, or out of step with its camera's counter

Flagged files appear under **Issues**; clicking a group selects them. In the preview pane you can
take the date from another source, type one in, or anchor one file to its true date and shift the
whole selection by that offset, keeping the gaps intact.

Corrections live in a `*.overrides.json` file beside the plan. They survive reopening, they are what
the copy uses, and any of them can be undone. A file that has already been copied refuses to be
re-dated.

## Seeing it rather than reading it

Four visual tabs. In all three walkable ones: click to take the mouse,
<kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> to move, <kbd>Shift</kbd> to run, mouse to look,
<kbd>Esc</kbd> to give the mouse back.

### Timeline

The whole plan in 3D, one point per date each source reported, coloured by confidence. Three
layouts, morphing between them:

| Layout | What it shows |
|---|---|
| **Disagreement field** | Time across, one lane per source, depth by how many files stack up. Agreeing sources collapse to a tick; EXIF 2003 against file system 2019 draws a long red streak. |
| **Time helix** | One turn per year, month around the turn, hour of day as radius. Bad dates lift out of the disc. |
| **Memory terrain** | Time across, hour of day in depth, file count as height. A reset date becomes a spire on an empty plain. |

Detail follows distance: zoomed out, one muted dot per file with only suspect dates keeping their
colour; closer, every source's reading and the full colour key; closer still, the photographs
themselves in place, with videos playing. Long empty stretches are compressed so a lonely 2003
island and a dense 2019 cluster are both readable on one axis. Clicking a point opens that file in
the preview pane. **Auto-fly** cruises from oldest to newest; any drag takes the controls back.

### Gallery

Builds a building out of your archive. One room per period, oldest first, a longer hall for a busy
year. Pictures hang at eye height, videos play in frame, daylight falls through a clerestory band.
Rooms open into one another as one enfilade. Look at a picture for its name and date; click or press
<kbd>E</kbd> to open it in the preview pane.

### Scene — walking into a single picture

A photograph is a pinhole camera's view of a space, so the space can be read back out. Mark the
point where the picture recedes and the rectangle of the furthest wall, and eonsort projects the
picture onto a five-sided box: back wall, floor, ceiling, two side walls.

The first fit is a guess. Drag the yellow cross onto the vanishing point and the four blue corners
onto the far wall; the room rebuilds as you drag, with dashed guides to line up against anything
receding in the photograph.

| Control | What it does |
|---|---|
| **lens** | How wide the taking lens was |
| **Reset** | Back to the guess, forgetting the saved fit |
| **Flat** | Collapse to a picture wall with a floor — the honest answer for a portrait or close-up |
| **Ask the model** | The local model fills in the handles; a starting point only, since vision models read bounding boxes well and vanishing points badly |
| **Walk in** | Take the mouse. Before you move, the view is the photograph itself. |

Room size follows from the photographer's eye being 1.7 m above the floor, so walking speed and head
height need no tuning. The plaque gives depth, width and height, and says so if the fit is barely
walkable. Fits are remembered per picture in `*.scenes.json` beside the plan.

Objects the model finds on the visible floor can be shown as **cut-outs** — pieces of the photograph
standing up with a contact shadow. Off by default: without a cut-out mask the subject also stays
painted on the floor and wall behind it, so you see it twice.

#### Real depth, optionally

With the optional `depth` feature, **Read depth** runs
[Depth Anything V2](https://depth-anything-v2.github.io/) locally and turns the photograph into a
displaced mesh, so a face stands out from the wall behind it. The **relief** slider fades between
the flat box and full displacement. Depth is *relative*, not metric — it says what is nearer, not
how far.

At a foreground edge the mesh has nothing behind it, so those triangles are dropped. **fill**
decides what shows through the gap:

| fill | What happens |
|---|---|
| `nearest` | Carries the background a short way behind each near edge, colour and distance both, as a second layer. No model, no network, instant. |
| `service` | Sends the carried band to an endpoint speaking OpenAI's `/v1/images/edits`. Press **Fill**. |
| `local` | Same band, painted in process by Stable Diffusion 1.5 inpainting. Needs the `diffuse` feature. |
| `none` | Leaves the gap open. |

`service` remembers four settings between sessions: **endpoint** (a bare host gains
`/v1/images/edits`; Automatic1111's reply shape is understood too), **key** (empty for a local
server), **model** and **size**. `local` takes only a **prompt**. Both receive the picture with the
torn band cut out as transparency.

Both painters supply *colour only* — distances stay the propagated background, so a painted patch
never invents geometry. Step far enough sideways and the flat room shows through anyway.

Neither feature is in the default build:

```sh
cargo tauri build --features depth              # depth mesh + nearest/service fill
cargo tauri build --features "depth diffuse"    # adds the local painter
```

Weights download on first use into `models/` in the app data folder — delete it to reclaim the
space — from pinned revisions of [`lmz/candle-dino-v2`](https://huggingface.co/lmz/candle-dino-v2),
[`jeroenvlek/depth-anything-v2-safetensors`](https://huggingface.co/jeroenvlek/depth-anything-v2-safetensors),
[`stable-diffusion-v1-5/stable-diffusion-inpainting`](https://huggingface.co/stable-diffusion-v1-5/stable-diffusion-inpainting)
and [`openai/clip-vit-base-patch32`](https://huggingface.co/openai/clip-vit-base-patch32).

| | Download | CPU cost |
|---|---|---|
| depth | ~190 MB | a second or two per picture |
| diffuse | ~2.1 GB | **minutes** per hole, at 20 steps and 512×512 |

Impatient machines should use `nearest`, or point `service` at something with a GPU. A build without
a feature says so instead of offering a button that cannot work. The `service` fill needs neither
feature nor weights — only a reachable endpoint.

### Charts

The same questions in flat 2D, often quicker to read:

| Figure | The question it answers |
|---|---|
| **When your files were made** | A square per month, year by year. Where are the gaps? Click in to go finer. |
| **Time of day** | Do the hours look like a camera roll? A midnight spike means dates with no time. |
| **Where each date came from** | Which source was trusted. A large "file system" share means many dates are really copy dates. |
| **How sure eonsort is** | The timeline's four colours, with what each means. |
| **Where the bulk lands** | The fullest destination folders — one outsized folder is usually where unknown dates collected. |

The first figure is also the way in. Click a square to drop into it, drag for a range, or click a
row label for a whole year; the grid refines from years to months to days to hours.

Whatever you pick scopes the whole app: every figure recounts itself, and Timeline, Gallery and
Scene show only those files. A bar under the toolbar tracks where you are — `All time › 2019 › Mar
2019` — each step clickable, plus **Back** and **Show all**. Scanning again clears it.

## Turning pictures upright

Cameras store a sideways photo the way the sensor saw it, with an orientation tag saying which way
is up. Plenty of software ignores the tag.

Tick **Turn pictures upright when copying** before scanning. The scan records what the copy should
do; the copy turns the pixels for real and sets the tag to "already upright", so the result looks
right everywhere. Originals are never touched.

For JPEGs the turn is genuinely lossless — compressed data is rearranged, not decoded and
recompressed — and the EXIF block survives intact.

The preview shows every picture as it will be copied, and any of them can be corrected:

| Key | |
|---|---|
| `[` | quarter turn left |
| `]` | quarter turn right |
| `\` | upside down |
| `0` | back to what the tag asked for |

The same buttons sit under the preview; selecting several files gives a bulk turn in the bottom bar.

Non-JPEGs, and JPEGs whose dimensions are not a whole number of compression blocks, cannot be turned
losslessly. They are copied untouched and the preview says so. A button will turn one anyway, naming
the cost: re-encoding and dropped metadata.

Turns live in `*.rotations.json` beside the plan, and a copied file refuses to be turned. One
caveat: the small thumbnail inside a photo's own EXIF block is left as it was, so a viewer showing
that thumbnail may still show it sideways.

## Safety

- **Nothing is overwritten.** Same-name files are compared by content; identical ones are left alone, different ones stored beside as `name_dup_1.jpg`.
- **Every copy is atomic.** Written to staging, flushed, then renamed into place.
- **Everything resumes.** The scan appends to its plan, the copy keeps a journal. Stopping a run, or losing power, costs nothing.
- **Sources are only ever read.**

## Quick Start

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

`--destination` is optional on `scan` — leave it out to see the folder layout before deciding where
it goes, and the plan records relative paths. Naming it on `copy` re-points the plan.

| Flag | Meaning |
|---|---|
| `--pattern "%Y/%m"` | Destination layout. Any strftime pattern, e.g. `%Y/%Y-%m-%d`. |
| `--destination "E:\Sorted"` | Where the sorted tree goes. Optional on `scan`; on `copy` it sets or changes where an existing plan lands. |
| `--provider filename exif` | Only these date sources, in this order. |
| `--strategy oldest` | Earliest date reported. `priority` stops at the first source that reports one. |
| `--suspect` | On `show`, list only entries whose date looks wrong. |
| `--auto-rotate` | On `scan`, note sideways pictures so the copy turns them upright. |
| `--jobs 8` | Copy this many files in parallel. |
| `--hash` | Compare contents during `verify`, not just sizes. |

Ctrl-C at any time; run the same command again to continue.

---

## How it is put together

```
crates/eonsort-core   the engine: date providers, scanning, planning, turning, copying, verifying
crates/eonsort-cli    the eonsort command line tool
src-tauri             the desktop app backend and its Tauri commands
src                   the SvelteKit front end
```

A **plan** is JSON Lines: one header record, then one per file with where it came from, every date
each source reported, anything suspect about the chosen one, and where it will go. The copy reads
the plan and writes its own journal beside it. Both are append-only, which is what makes every phase
restartable.

Your decisions stay out of the plan, in sidecars beside it — `*.overrides.json` for dates,
`*.rotations.json` for turns, `*.scenes.json` for fitted rooms — so what was *detected* is never
overwritten by what you *decided*. Everything that loads a plan by path applies the sidecars, so app
and CLI agree.

Turning a picture changes its bytes, so the copy journals the size and hash of what it wrote.
`verify` and the duplicate check compare against that record rather than the source, so a second
copy run recognises the turned file instead of writing `name_dup_1.jpg`.

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first.

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run check && npm test
```

The optional features are not in that line; CI checks them in separate non-blocking jobs. If you
touch `depth.rs` or `diffuse.rs`:

```sh
cargo clippy -p eonsort-core --features depth --all-targets -- -D warnings && cargo test -p eonsort-core --features depth
cargo clippy -p eonsort-core --features diffuse --all-targets -- -D warnings && cargo test -p eonsort-core --features diffuse
```

`inpaint.rs` — the `service` fill — needs no feature and is covered by the default line.

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

`npm run test:e2e` builds a debug binary, launches it under `tauri-driver`, and runs the specs in
`tests/e2e/specs`. **Close any running eonsort window first** — the rebuild cannot replace a locked
`.exe`, and the suite will then quietly run the previous binary and pass without testing your
changes.

---

## License

MIT © 2026 Georg Nelles
