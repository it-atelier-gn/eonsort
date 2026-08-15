# eonsort

[![Build](https://github.com/it-atelier-gn/eonsort/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/eonsort/actions)
[![Rust](https://img.shields.io/badge/rust-1.87%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Sorts files into date-based folders using the date they were actually created, not the date a backup tool last touched them.

Point it at your photo folders, press **Scan**, and you get a full preview of the resulting tree before a single byte is copied. Browse the planned folders, click any file to see it, then press **Copy files**. Sources are never modified.

Runs on Windows, Linux and macOS, as a desktop app and as an `eonsort` command line tool.

![Eonsort desktop app](docs/screenshot.png)

---

## How the date is found

Four sources are consulted:

| Source | What it reads |
|---|---|
| **File name** | `IMG_20230506_101112.jpg`, `Screenshot 2021-07-04 at 09.05.01.png`, `Rechnung 06.05.2021.pdf`, and many more shapes |
| **EXIF** | `DateTimeOriginal` and friends in JPEG, TIFF, PNG, WebP, HEIF and common raw formats |
| **Media** | The recording time in the `mvhd` box of MP4, MOV, M4A and related containers |
| **File system** | Whichever of the created / modified timestamps is older |

Every source that reports a date is kept, so you can always see what each one said. Three rules are
available for choosing between them:

| Rule | What it does |
|---|---|
| **Weigh the evidence** (default) | Throws out dates that cannot be true, then prefers the one two sources agree on |
| **Oldest date wins** | Keeps the earliest date any source reports |
| **First match wins** | Walks the sources in order and stops at the first hit |

Each source can also be switched off entirely.

### The optional local model

A fifth source reads the date **printed into the picture itself** — the orange stamp a film camera
burned into the corner, or a date written on a scanned document. It needs a model runner you host,
so nothing ever leaves your machine.

Point eonsort at [Ollama](https://ollama.com) (or any OpenAI-compatible endpoint) and press **Check
connection**. It reports whether the runner answers and whether each model you named is installed.
With Ollama it can also manage them for you:

- **Download** pulls the model, with a live progress bar and a **Stop** button. Nothing is installed
  unless the pull finishes.
- **Remove** deletes it again, behind a "Really remove?" confirmation.

Both buttons are Ollama-only — an OpenAI-compatible endpoint has no install API, and eonsort says so
instead of pretending. Reading pictures is off by default and roughly a second per image, so it is
used only on the files you ask about unless you turn on "Look at every picture during the scan".

## When the date is wrong

A camera whose battery died resets its clock to a factory default — usually 1 January of 2000, 2003
or 2015 — and every photo after that carries a date that *looks* real. Eonsort looks for the
signatures of that and other bad dates, and marks the files rather than quietly filing them under
the wrong year:

- a date sitting on a factory-reset instant, or in the future, or later than the file was written
- a whole run of files counting up from a reset date while the files themselves were written years later
- a batch sharing one timestamp down to the second
- a file stranded years away from everything else in its folder, or out of step with its camera's counter

Anything flagged shows up under **Issues**, and clicking a group selects those files. In the preview
pane you can then take the date from a different source, type one in, or — for a camera whose clock
was wrong for an entire trip — anchor one file to its true date and shift the whole selection by
that same offset, keeping the gaps between the shots intact.

### Seeing it rather than reading it

The **Timeline** tab draws the whole plan in 3D, one point per date each source reported, coloured by
how much the date can be trusted. It has three layouts and morphs between them:

| Layout | What it shows |
|---|---|
| **Disagreement field** | Time across, one lane per date source, depth by how many files stack up. A file whose sources agree collapses to a tight tick; one whose EXIF says 2003 and whose file system says 2019 draws a long red streak across the whole timeline. |
| **Time helix** | One turn per year, month around the turn, hour of day as radius. Files with a bad date lift out of the disc. |
| **Memory terrain** | Time across, hour of day in depth, file count as height. A reset date becomes a needle-thin spire on an empty plain. |

Each layout carries a card saying what its axes mean and what to look for, and the colour key sits
beside it — no chart here expects you to already know how to read it.

**How much you are shown depends on how close you are.** Zoomed out, the archive is one dot per
file in muted slate, and only the dates that look wrong keep their colour and their threads — so
the trouble is the only thing that stands out in a hundred thousand files. Come closer and every
source's reading fades in and the full colour key returns. Closer still and the files themselves
appear on the nearest points. The panel on the right names the level you are at.

Long empty stretches of time are compressed, so a lonely 2003 island and a dense 2019 cluster are
both readable on one axis instead of collapsing into two dots. Clicking any point opens that file in
the preview pane, where the date can be corrected.

**Zoom in far enough and the points become the files themselves** — photographs appear in place, and
videos play. Keep zooming out and they fade back to points, so the whole archive stays readable at a
distance and browsable up close.

**Auto-fly** cruises the camera through the whole archive at speed, from the oldest file to the
newest, diving close enough that the pictures stream past. Any drag, pan or scroll takes the
controls straight back.

### The gallery

The **Gallery** tab builds an actual building out of your archive and lets you walk through it.

Each period gets its own room, oldest first, and a busy year gets a longer hall. Pictures hang at
eye height on both walls, videos play in their frames, daylight falls through a clerestory window
band, and there are benches, plinths and planters to walk around. The rooms open into one another,
so the whole archive is one enfilade you can walk end to end.

Click to take the mouse, then <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> to move,
<kbd>Shift</kbd> to run, and the mouse to look. Look at any picture and its name and date appear;
click or press <kbd>E</kbd> to open it in the preview pane. <kbd>Esc</kbd> gives the mouse back.

### Walking into a single picture

The **Scene** tab takes one photograph and turns it into a room you can walk around inside.

A photograph is a pinhole camera's view of a space, so the space can be read back out of it. Mark
the point where the picture recedes — the far end of a corridor, the vanishing point of a street —
and the rectangle of the furthest wall facing you, and eonsort projects the picture onto a five-sided
box: back wall, floor, ceiling and two side walls. Walk forward and the walls slide past you with
the parallax they had when the shutter opened.

The first fit is only a guess. Drag the yellow cross onto the vanishing point and the four blue
corners onto the far wall, and the room rebuilds as you drag. The dashed guides run from the corners
of the picture to the corners of the wall, so you can line them up with anything in the photograph
that recedes. The **lens** slider sets how wide the taking lens was, **Reset** returns to the guess,
and **Flat** collapses the box to a picture wall with a floor — the honest answer for a portrait or
a close-up, which has no perspective to walk into.

The room's size is read off the assumption that the photographer's eye was 1.7 m above the floor, so
walking speed and head height feel right without anything to tune. The plaque gives the room's depth,
width and height; if the fit produces something barely walkable it says so rather than refusing.

Press **Walk in** to take the mouse, then <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> to move,
<kbd>Shift</kbd> to run, and the mouse to look. <kbd>Esc</kbd> steps back out to the fitting view.
Before you move, the view is the photograph itself. The filmstrip along the bottom switches pictures
and keeps the preview pane in step.

Your fit is remembered per picture in a `*.scenes.json` file beside the plan, so a room you took
trouble over is still there when you come back. **Reset** forgets it again.

If the local model is switched on, **Ask the model** hands it the picture and asks where the
perspective goes and what is standing in front of it. The answer is only ever a starting point —
it fills in the handles and leaves you in the fitting view to correct it. A vision model reads
bounding boxes well and vanishing points badly, so expect to drag.

Anything the model finds standing on the visible floor can be shown as a **cut-out**: a piece of the
photograph standing up in the room with a contact shadow, which you walk around instead of through.
This is off by default and honestly imperfect — without a cut-out mask the subject also stays
painted on the floor and wall behind it, so you see it twice. The app says so on screen rather than
pretending otherwise.

#### Real depth, optionally

The box is a guess about a room. A build made with the optional `depth` feature can do better:
**Read depth** runs [Depth Anything V2](https://depth-anything-v2.github.io/) locally and turns the
photograph into a displaced mesh, so a face really does stand out from the wall behind it. The
**relief** slider fades between the flat box and the full displacement.

At the edges of a foreground object the mesh has nothing behind it to show, so those triangles are
dropped. The **fill** control decides what you see through the gap, and offers four answers:

| fill | What happens |
|---|---|
| `nearest` | The background is carried a short way behind each near edge — colour and distance both — and drawn as a second layer, so a head no longer has the flat wall showing through beside it. No model, no network, instant. |
| `service` | The carried band is sent to an image-editing endpoint that speaks OpenAI's `/v1/images/edits`, which paints what was hidden. Press **Fill** to run it. |
| `local` | The same band, painted in-process by Stable Diffusion 1.5 inpainting. Needs the optional `diffuse` feature and its weights. |
| `none` | The gap is left open, which is what earlier builds did. |

`service` shows four settings, remembered between sessions: **endpoint** (a bare host gains
`/v1/images/edits`; `http://localhost:8080` reaches most local servers, and Automatic1111's reply
shape is understood too), **key** (leave empty for a local server), **model**, and **size**. `local`
shows only a **prompt**. Both are given the picture with the torn band cut out as transparency, and
both are asked to invent only what the band covers.

Both painters supply *colour only*. The distances in the band stay the propagated background, so a
painted patch never invents geometry — it just stops the flat room showing through. Step far enough
sideways and it does anyway.

One honest limit remains. The depth is *relative*, not measured in metres — it says what is nearer,
not how far.

This is off the beaten path deliberately. It adds a large dependency and roughly 190 MB of model
weights, so it is not in the default build:

```sh
cargo tauri build --features depth      # or: cargo build -p eonsort-desktop --features depth
```

The weights download on first use into `models/` inside the app data folder — delete that folder to
get the space back — from pinned revisions of
[`lmz/candle-dino-v2`](https://huggingface.co/lmz/candle-dino-v2) and
[`jeroenvlek/depth-anything-v2-safetensors`](https://huggingface.co/jeroenvlek/depth-anything-v2-safetensors).
Inference runs on the CPU and takes a second or two per picture. A build without the feature says so
plainly instead of offering a button that cannot work.

The `service` fill needs no feature and no weights — only a reachable endpoint. The `local` fill
needs a second optional feature, which brings the diffusion model in alongside the depth one:

```sh
cargo tauri build --features "depth diffuse"
```

**Get painting model** downloads about 2.1 GB into the same `models/` folder, from pinned revisions
of [`stable-diffusion-v1-5/stable-diffusion-inpainting`](https://huggingface.co/stable-diffusion-v1-5/stable-diffusion-inpainting)
and the CLIP tokenizer from
[`openai/clip-vit-base-patch32`](https://huggingface.co/openai/clip-vit-base-patch32). Be plain
about the cost: twenty denoising steps at 512×512 on a CPU take **minutes**, not seconds. A machine
without a lot of patience should use `nearest`, or point `service` at something with a GPU.

The **Charts** tab answers the same questions in flat 2D, which is often quicker to read:

| Figure | The question it answers |
|---|---|
| **When your files were made** | A square per month, year by year. Where are the gaps, and is anything stranded far from the rest? Click into it to go finer. |
| **Time of day** | Do the hours look like a camera roll? A spike at midnight means those files carry a date with no time. |
| **Where each date came from** | Which source was trusted. A large "file system" share means many dates are really copy dates. |
| **How sure eonsort is** | The same four colours the timeline uses, with what each one actually means. |
| **Where the bulk lands** | The fullest destination folders — one outsized folder is usually where unknown dates collected. |

Every figure says in one line what it shows and what is worth noticing in it.

**Digging into a stretch of time.** The first figure is also the way in. Click a square to drop into
it, drag across squares to take an arbitrary range, or click the label on the left for a whole row —
and the grid gets finer as you go: months of the years, then days of a month, then hours of a day.

Whatever you pick becomes the scope for the whole app. Every other figure recounts itself for it, and
the **Timeline**, **Gallery** and **Scene** tabs show only those files, so you can spot an odd
fortnight in the heat map and then walk through exactly those pictures. A bar under the toolbar
tracks where you are — `All time › 2019 › Mar 2019` — with each step clickable to come back up, plus
**Back** and **Show all**. Scanning again clears it.

Your corrections live in a `*.overrides.json` file beside the plan. They survive reopening the plan,
they are what the copy actually uses, and you can undo any of them. Nothing rewrites the plan
itself, and a file that has already been copied refuses to be re-dated.

## Turning pictures upright

Cameras and phones rarely rotate a photo when you hold them sideways. They store it the way the
sensor saw it and add an orientation tag saying which way is up. Plenty of software ignores that
tag, so a sorted archive ends up full of pictures lying on their side.

Tick **Turn pictures upright when copying** before you scan. The scan then reads each picture's
orientation tag and records what the copy should do about it. When the copy runs it turns the
pixels for real and sets the tag to "already upright", so the result looks right in everything —
tag-aware or not. Your originals are never touched.

For JPEGs the turn is genuinely lossless: the compressed data is rearranged rather than decoded and
re-compressed, so nothing degrades no matter how often you turn a picture. The EXIF block survives
intact, dates and camera details included.

The preview shows every picture the way it will be copied, and you can correct any of them:

| | |
|---|---|
| `[` | turn a quarter to the left |
| `]` | turn a quarter to the right |
| `\` | turn upside down |
| `0` | back to the orientation the tag asked for |

The same buttons sit under the preview, and selecting several files at once gives you a bulk turn
in the bar along the bottom.

Some pictures cannot be turned losslessly — anything that is not a JPEG, and JPEGs whose width or
height is not a whole number of compression blocks. Those are copied exactly as they are and the
preview says so. If you would rather have such a picture turned anyway, there is a button for it
that spells out the cost: it re-encodes the image and drops its metadata. Nothing takes that path
unless you ask for it by name.

Your turns live in a `*.rotations.json` file beside the plan, next to your date corrections, and a
file that has already been copied refuses to be turned. One caveat worth knowing: the small preview
thumbnail embedded inside a photo's own EXIF block is left as it was, so a viewer that shows you
that thumbnail rather than the picture may still show it sideways.

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
- [CMake](https://cmake.org/) and [NASM](https://nasm.us/), which build the bundled libjpeg-turbo used to turn pictures losslessly
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

`--destination` is optional on `scan`. Leave it out to see the folders your files would be grouped
into before you decide where they go; the plan records them as relative paths. Name the destination
when you copy, and the plan is re-pointed at it:

```sh
eonsort scan --source "D:\Photos" --plan photos.jsonl
eonsort copy --plan photos.jsonl --destination "E:\Sorted"
```

List only the entries whose date looks wrong, with the reason for each:

```sh
eonsort show --plan photos.jsonl --suspect
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
| `--destination "E:\Sorted"` | Where the sorted tree goes. Optional on `scan`; on `copy` it sets or changes where an existing plan lands. |
| `--provider filename exif` | Only consult these date sources, in this order. |
| `--strategy oldest` | Take the earliest date reported instead of weighing the evidence. `priority` stops at the first source that reports one. |
| `--suspect` | On `show`, list only entries whose date looks wrong. |
| `--auto-rotate` | On `scan`, note which pictures are sideways so the copy turns them upright. Lossless for JPEGs; anything it cannot turn without re-encoding is copied untouched. |
| `--jobs 8` | Copy this many files in parallel. |
| `--hash` | Compare file contents during `verify`, not just sizes. |

Press Ctrl-C at any time. Run the same command again to continue.

---

## How it is put together

```
crates/eonsort-core   the engine: date providers, scanning, planning, turning, copying, verifying
crates/eonsort-cli    the eonsort command line tool
src-tauri             the desktop app backend and its Tauri commands
src                   the SvelteKit front end
```

A **plan** is a JSON Lines file: one header record, then one record per file describing where it came from, every date each source reported, anything that looks wrong about the one that was chosen, and where the file will go. The copy step reads the plan and writes its own journal next to it. Both files are append-only, which is what makes every phase restartable.

Date corrections are kept out of the plan, in a `*.overrides.json` sidecar beside it, so the record of what was *detected* is never overwritten by what you *decided*. Rotations you make by hand live the same way in `*.rotations.json`, and the rooms you fit to a photograph in `*.scenes.json`. Everything that loads a plan by path applies the sidecars, so the desktop app and the command line agree on where a file goes and which way up it lands.

Turning a picture changes its bytes, so the copy records the size and hash of what it actually wrote into the journal. `verify` compares a turned copy against that record rather than against the source, and the duplicate check does the same — running a copy twice recognises the turned file instead of writing it again as `name_dup_1.jpg`.

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first to discuss the approach.

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run check && npm test
```

The optional `depth` and `diffuse` features are not part of that line, and CI only checks them in a
separate non-blocking job. If you touch `crates/eonsort-core/src/depth.rs` or
`crates/eonsort-core/src/diffuse.rs`, build them yourself:

```sh
cargo clippy -p eonsort-core --features depth --all-targets -- -D warnings && cargo test -p eonsort-core --features depth
cargo clippy -p eonsort-core --features diffuse --all-targets -- -D warnings && cargo test -p eonsort-core --features diffuse
```

`crates/eonsort-core/src/inpaint.rs` — the `service` fill — needs no feature and is covered by the
default line above.

The application icon is generated rather than hand-drawn. To change it, edit `scripts/make-icon.mjs` and run `npm run icon`.

### End-to-end UI tests

`tests/e2e` drives the actual desktop window through [WebdriverIO](https://webdriver.io/) and
[`tauri-driver`](https://crates.io/crates/tauri-driver), rather than just type-checking the front
end. Supported on Windows and Linux (`tauri-driver` has no macOS WebDriver backend).

One-time setup:

```sh
cargo install tauri-driver --locked
```

- **Windows**: nothing else to install — `npm run test:e2e` downloads the Microsoft Edge WebDriver
  build matching the installed WebView2 Runtime automatically.
- **Linux**: install `webkit2gtk-driver` (Debian/Ubuntu) or your distro's equivalent, so that
  `WebKitWebDriver` is on `PATH`.

Then run:

```sh
npm run test:e2e
```

This builds a debug binary (`tauri build --debug --no-bundle`), launches it under `tauri-driver`,
and runs the specs in `tests/e2e/specs`.

Close any running eonsort window first. The rebuild cannot replace a locked `.exe`, and the suite
will then quietly run the *previous* binary and pass without testing your changes.

---

## License

MIT © 2026 Georg Nelles
