import { mkdirSync, writeFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const API = "https://commons.wikimedia.org/w/api.php";
const AGENT =
  "eonsort-sample-fetcher/1.0 (https://github.com/it-atelier-gn/eonsort)";
const LICENCE = "CC0";
const CATEGORIES = ["Featured pictures on Wikimedia Commons", "Quality images"];
const PAUSE_MS = 1200;
const MAX_BYTES = 12_000_000;

const asked = process.argv.slice(2);
if (asked.some((a) => a === "-h" || a === "--help")) {
  console.log("usage: node scripts/fetch-sample-pictures.mjs [folder] [count]");
  console.log(
    "Fetches CC0 pictures from Wikimedia Commons, each carrying a date of its own.",
  );
  process.exit(0);
}

const out = asked[0] ?? "samples";
const wanted = Number(asked[1] ?? 20);
if (!Number.isInteger(wanted) || wanted < 1) {
  console.error(`${asked[1]} is not a count`);
  process.exit(1);
}

const rest = () => new Promise((go) => setTimeout(go, PAUSE_MS));

async function ask(params) {
  const url = new URL(API);
  for (const [key, value] of Object.entries({ format: "json", ...params })) {
    url.searchParams.set(key, value);
  }
  const answer = await fetch(url, { headers: { "User-Agent": AGENT } });
  if (!answer.ok) {
    throw new Error(
      `${answer.status} ${answer.statusText} for ${params.gsrsearch ?? ""}`,
    );
  }
  return answer.json();
}

async function candidates(category, limit, offset) {
  const body = await ask({
    action: "query",
    generator: "search",
    gsrsearch: `incategory:"${category}" incategory:"CC-Zero" filetype:bitmap`,
    gsrnamespace: 6,
    gsrlimit: limit,
    gsroffset: offset,
    prop: "imageinfo",
    iiprop: "url|size|mime|extmetadata",
    iiextmetadatafilter:
      "LicenseShortName|Artist|ImageDescription|DateTimeOriginal|GPSLatitude",
  });
  return Object.values(body.query?.pages ?? {});
}

function plain(html) {
  return (html ?? "")
    .replace(/<[^>]*>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();
}

function shotAt(value) {
  const said = plain(value);
  if (!said || said.includes("QS:P") || /circa|century/i.test(said))
    return null;
  const stamped = said.match(
    /(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}:\d{2}:\d{2}))?/,
  );
  if (stamped)
    return stamped[4]
      ? `${stamped[1]}-${stamped[2]}-${stamped[3]} ${stamped[4]}`
      : null;
  const written = said.match(/(\d{1,2}) (\w+) (\d{4}), (\d{2}:\d{2}:\d{2})/);
  if (!written) return null;
  const month = new Date(`${written[2]} 1, 2000`).getMonth();
  if (Number.isNaN(month)) return null;
  const pad = (n) => String(n).padStart(2, "0");
  return `${written[3]}-${pad(month + 1)}-${pad(written[1])} ${written[4]}`;
}

function keeps(page) {
  const shot = page.imageinfo?.[0];
  if (!shot || shot.mime !== "image/jpeg") return false;
  if (shot.size > MAX_BYTES) return false;
  if (plain(shot.extmetadata?.LicenseShortName?.value) !== LICENCE)
    return false;
  return shotAt(shot.extmetadata?.DateTimeOriginal?.value) !== null;
}

function exifIn(bytes) {
  if (bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) return null;
  let at = 2;
  while (at + 4 <= bytes.length) {
    if (bytes[at] !== 0xff) return null;
    const marker = bytes[at + 1];
    if (marker === 0xd9 || marker === 0xda) return null;
    const length = bytes.readUInt16BE(at + 2);
    if (length < 2) return null;
    const body = bytes.subarray(at + 4, at + 2 + length);
    if (
      marker === 0xe1 &&
      body.subarray(0, 6).toString("latin1") === "Exif\0\0"
    ) {
      return body.subarray(6);
    }
    at += 2 + length;
  }
  return null;
}

function tagsOf(tiff) {
  const order = tiff.subarray(0, 2).toString("latin1");
  if (order !== "II" && order !== "MM") return new Set();
  const big = order === "MM";
  const short = (o) => (big ? tiff.readUInt16BE(o) : tiff.readUInt16LE(o));
  const long = (o) => (big ? tiff.readUInt32BE(o) : tiff.readUInt32LE(o));
  if (short(2) !== 42) return new Set();

  const seen = new Set();
  const gps = new Set();
  const walk = (start, depth, inGps) => {
    if (depth > 2 || start + 2 > tiff.length) return;
    const count = short(start);
    for (let i = 0; i < count; i += 1) {
      const entry = start + 2 + i * 12;
      if (entry + 12 > tiff.length) return;
      const tag = short(entry);
      (inGps ? gps : seen).add(tag);
      if (tag === 0x8769) walk(long(entry + 8), depth + 1, false);
      if (tag === 0x8825) walk(long(entry + 8), depth + 1, true);
    }
  };
  walk(long(4), 0, false);
  return { seen, gps };
}

function readsAs(bytes) {
  const tiff = exifIn(bytes);
  if (!tiff) return { dated: false, located: false };
  const { seen, gps } = tagsOf(tiff);
  return {
    dated: seen.has(0x9003),
    located: gps.has(0x0002) && gps.has(0x0004),
  };
}

function nameOf(title) {
  return title
    .replace(/^File:/, "")
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(-80);
}

async function main() {
  mkdirSync(out, { recursive: true });
  const manifestPath = join(out, "manifest.json");
  const held = existsSync(manifestPath)
    ? JSON.parse(readFileSync(manifestPath, "utf8"))
    : { licence: LICENCE, source: "Wikimedia Commons", pictures: [] };
  const already = new Set(held.pictures.map((p) => p.title));

  const missing = Math.max(0, wanted - held.pictures.length);
  if (missing === 0) {
    console.log(
      `${held.pictures.length} pictures already in ${out}. Nothing to fetch.`,
    );
    return;
  }

  const perHand = Math.max(3, Math.ceil(missing / 8));
  const hands = new Map();
  for (const p of held.pictures) {
    hands.set(p.author, (hands.get(p.author) ?? 0) + 1);
  }

  const picked = [];
  for (const category of CATEGORIES) {
    for (
      let offset = 0;
      offset < 500 && picked.length < missing;
      offset += 100
    ) {
      const found = await candidates(category, 100, offset);
      if (found.length === 0) break;
      for (const page of found) {
        if (picked.length >= missing) break;
        if (already.has(page.title) || !keeps(page)) continue;
        const hand =
          plain(page.imageinfo[0].extmetadata?.Artist?.value) || "unknown";
        if ((hands.get(hand) ?? 0) >= perHand) continue;
        hands.set(hand, (hands.get(hand) ?? 0) + 1);
        picked.push({ page, category });
        already.add(page.title);
      }
      await rest();
    }
    if (picked.length >= missing) break;
  }

  if (picked.length === 0) {
    console.log("Nothing new to fetch.");
    return;
  }

  for (const { page, category } of picked) {
    const shot = page.imageinfo[0];
    const file = nameOf(page.title);
    const target = join(out, file);

    const answer = await fetch(shot.url, { headers: { "User-Agent": AGENT } });
    if (!answer.ok) {
      console.log(`  skipped ${file}: ${answer.status}`);
      await rest();
      continue;
    }
    const bytes = Buffer.from(await answer.arrayBuffer());
    const reads = readsAs(bytes);
    if (!reads.dated) {
      console.log(`  skipped ${file}: the file carries no date of its own`);
      await rest();
      continue;
    }
    writeFileSync(target, bytes);

    const meta = shot.extmetadata ?? {};
    held.pictures.push({
      file,
      title: page.title,
      page: `https://commons.wikimedia.org/wiki/${encodeURIComponent(page.title)}`,
      author: plain(meta.Artist?.value) || "unknown",
      licence: plain(meta.LicenseShortName?.value),
      category,
      taken: shotAt(meta.DateTimeOriginal?.value),
      located: reads.located,
    });

    const stamp = held.pictures.at(-1);
    console.log(`  ${file}  ${stamp.taken}${stamp.located ? "  GPS" : ""}`);
    await rest();
  }

  held.pictures.sort((a, b) => a.file.localeCompare(b.file));
  writeFileSync(manifestPath, `${JSON.stringify(held, null, 2)}\n`);

  const located = held.pictures.filter((p) => p.located).length;
  console.log(
    `\n${held.pictures.length} pictures in ${out}, ${located} of them with a reading.`,
  );
  console.log(`Every one carries a date of its own and is ${LICENCE}.`);
  console.log(`${manifestPath} says where each came from.`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
