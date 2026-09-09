import fs from "fs";
import path from "path";

const NEWLINE = String.fromCharCode(10);

const BASE_JPEG =
  "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAoHBwkHBgoJCAkLCwoMDxkQDw4ODx4WFxIZJCAmJSMgIyIoLTkwKCo2KyIjMkQy" +
  "Njs9QEBAJjBGS0U+Sjk/QD3/2wBDAQsLCw8NDx0QEB09KSMpPT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09" +
  "PT09PT09PT09PT09PT3/wAARCABIAGADASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAA" +
  "AgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6" +
  "Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXG" +
  "x8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREA" +
  "AgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5" +
  "OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPE" +
  "xcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDnqKKK9Y4QooooAKKKnWxunUMttMVIyCIz" +
  "giplOMfidhqLexBRVj+z7v8A59Z/+/ZqvSjOM/hdwcWt0FFFFWIKKKKACiiigArT0vRnvx5sjGOEHrjlvXH+NVtNs/t16kJL" +
  "BTksVGcAf5x+NdoqhFCqAFAwABwBXh5vmUsMlSpfE+vZHfg8Kqvvz2RDbWFtZ5+zwqhPU9T+ZqeiivkZzlN803d+Z7MYqKsk" +
  "FQ3NpBdptuIlcDpnqPoe1TUUoylB80XZg0mrM5nVdCNqrT2xLRA5Kd0H9RWNXf1yOt2Isr4+WD5cg3jjABzyB/nuK+ryfM5V" +
  "37Gq7vo+55GNwqp+/DYzqKKK+gPOCiiigDZ8M4+3y8nd5RwMcYyP/rV01cbpN4LLUEkckRkFXwM8H/6+K7Kvjc+pyjied7Nf" +
  "ke3l8k6VuwUUUV4h3BRRRQAVgeKf+XX/AIH/AOy1v1yviC8FzfCNCSkIKnIx82ef6flXr5JTlLFxktle/wB1jjx0kqLT6mVR" +
  "RRX2x4QUUUUAFbWk659mQQXW5oxwjjkr7H2rFornxOGp4mHJUWhpSqypS5oneRTRzruikSRQcZVgRmn1wcU0kDbopHjYjGVY" +
  "g4q4ut6gqhRcHAGOVUn88V85V4eqJ/upprz0/wAz04ZlG3vI7CmySJEheR1RR1ZjgCuS/t3UP+fj/wAcX/CqctxNPjzpZJMd" +
  "N7E4pUuHqrf7yaS8rv8AyCWYwt7qZu6rrw2tBZMd2cNKOmP9n/H8vWueoor6LCYOlhYclNer6s82tWlWlzSCiiiuoyCiiigA" +
  "ooooAKKKKACiiigAooooAKKKKAP/2Q==";

const TAKEN = takenList();

function takenList() {
  const out = [];
  for (let year = 2016; year <= 2023; year += 1) {
    for (let month = 1; month <= 12; month += 1) {
      const day = ((year + month) % 27) + 1;
      for (let shot = 0; shot < 2; shot += 1) {
        const hour = 8 + ((month + shot) % 11);
        const minute = (year + month * 7 + shot * 23) % 60;
        out.push(
          [
            String(year),
            "-",
            String(month).padStart(2, "0"),
            "-",
            String(day).padStart(2, "0"),
            "T",
            String(hour).padStart(2, "0"),
            ":",
            String(minute).padStart(2, "0"),
            ":",
            String((shot * 17 + month) % 60).padStart(2, "0"),
          ].join(""),
        );
      }
    }
  }
  return out;
}

function stamp(taken) {
  return taken.replace(/[-:]/g, "").replace("T", "_").slice(0, 15);
}

function picture(name) {
  const comment = Buffer.from(name, "utf8");
  const marker = Buffer.alloc(4);
  marker.writeUInt16BE(0xfffe, 0);
  marker.writeUInt16BE(comment.length + 2, 2);
  const base = Buffer.from(BASE_JPEG, "base64");
  return Buffer.concat([base.subarray(0, 2), marker, comment, base.subarray(2)]);
}

function entry(source, destination, taken, size) {
  return {
    kind: "entry",
    source,
    destination,
    taken,
    provider: "filename",
    provider_info: "IMG_yyyymmdd_hhmmss",
    size,
    candidates: [{ provider: "filename", info: "IMG_yyyymmdd_hhmmss", taken }],
    flags: [],
    subject: null,
    place: { city: null, region: null, country: null, country_code: null },
    tags: [],
    caption: null,
    orientation: 1,
    rotate: "none",
    rotate_reason: null,
    reencode: false,
  };
}

export function build(root) {
  const home = path.join(root, "home");
  const pictures = path.join(root, "pictures");
  const sorted = path.join(root, "sorted");
  const plans = path.join(home, "plans");
  fs.rmSync(root, { recursive: true, force: true });
  fs.mkdirSync(pictures, { recursive: true });
  fs.mkdirSync(plans, { recursive: true });
  fs.mkdirSync(sorted, { recursive: true });

  const lines = [];
  for (const taken of TAKEN) {
    const name = `IMG_${stamp(taken)}.jpg`;
    const source = path.join(pictures, name);
    const bytes = picture(name);
    fs.writeFileSync(source, bytes);
    const folder = path.join(sorted, taken.slice(0, 4), taken.slice(5, 7));
    lines.push(entry(source, path.join(folder, name), taken, bytes.length));
  }

  const header = {
    kind: "header",
    version: 4,
    created_at: TAKEN[0],
    sources: [pictures],
    destination: sorted,
    folder_pattern: "%Y/%m",
    name_pattern: "{original_name}",
    detect: {
      providers: ["gps", "xmp", "exif", "media", "takeout", "system", "filename", "filesystem"],
      strategy: "smart",
      weights: {},
    },
  };

  const plan = path.join(plans, "plan-e2e.jsonl");
  fs.writeFileSync(plan, [header, ...lines].map((r) => JSON.stringify(r)).join(NEWLINE) + NEWLINE);

  fs.writeFileSync(
    path.join(home, "settings.json"),
    JSON.stringify(
      {
        sources: [pictures],
        destination: sorted,
        folder_pattern: "%Y/%m",
        name_pattern: "{original_name}",
        providers: ["gps", "xmp", "exif", "media", "takeout", "system", "filename", "filesystem"],
        strategy: "smart",
        weights: {},
        follow_symlinks: false,
        auto_rotate: false,
        pair_companions: true,
        tag_pictures: false,
        rate_quality: false,
        find_faces: false,
        name_places: false,
        preserve_times: true,
        stamp_date: false,
        write_sidecars: false,
        compare_hashes: false,
        last_plan: plan,
      },
      null,
      2,
    ),
  );

  return { home, pictures, sorted, plan, count: TAKEN.length };
}

export function clear(root) {
  fs.rmSync(root, { recursive: true, force: true });
}
