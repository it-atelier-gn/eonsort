const call = (command, args) =>
  browser.executeAsync(
    function (command, args, done) {
      window.__TAURI_INTERNALS__.invoke(command, args).then(
        function (value) {
          done({ ok: true, value: value });
        },
        function (error) {
          done({ ok: false, error: String(error) });
        },
      );
    },
    command,
    args,
  );

async function value(command, args) {
  const result = await call(command, args);
  if (!result.ok) throw new Error(`${command} failed: ${result.error}`);
  return result.value;
}

async function rejection(command, args) {
  try {
    const result = await call(command, args);
    return result.ok ? null : result.error;
  } catch (error) {
    return String(error.message || error);
  }
}

async function settle(settings) {
  await value("save_settings", { settings });
  await browser.refresh();
  await (await $("#copy-settings")).waitForExist({ timeout: 10000 });
}

describe("The models behind tagging", () => {
  let original = null;

  before(async () => {
    original = await value("get_settings", {});
  });

  after(async () => {
    if (original) {
      await call("save_settings", { settings: original });
      await browser.refresh();
    }
  });

  it("reports what each model would cost and how much of it is here", async () => {
    for (const command of ["tag_model_status", "quality_model_status"]) {
      const status = await value(command, {});
      expect(typeof status.present).toBe("boolean");
      expect(typeof status.built_in).toBe("boolean");
      expect(status.total).toBeGreaterThan(100 * 1024 * 1024);
      expect(status.bytes).toBeLessThanOrEqual(status.total);
      expect(status.present).toBe(status.bytes === status.total);
    }
  });

  it("asks for more than twice as much for tagging as for judging", async () => {
    const tagging = await value("tag_model_status", {});
    const quality = await value("quality_model_status", {});
    expect(tagging.total).toBeGreaterThan(quality.total * 2);
  });

  it("stops an install this build cannot do rather than pretending", async () => {
    const tagging = await value("tag_model_status", {});
    if (tagging.built_in) return;

    expect(await rejection("install_tag_model", {})).toMatch(/without the tagging model/i);
    expect(await rejection("start_tagging", {})).toMatch(/without the tagging model/i);
    expect(await rejection("install_quality_model", {})).toMatch(/without the quality model/i);
  });

  it("takes a stop for a download that is not running without complaining", async () => {
    expect(await rejection("cancel_tag_install", {})).toBe(null);
    expect(await rejection("cancel_quality_install", {})).toBe(null);
  });

  it("keeps the tagging offer out of sight until tagging is asked for", async () => {
    await settle({ ...original, tag_pictures: false, rate_quality: false });
    expect(await $$("button=Get the tagging model")).toHaveLength(0);
    expect(await $$("*=judge how good each picture looks")).toHaveLength(0);
  });

  it("offers the tagging model, or says the build has none, once tagging is on", async () => {
    await settle({ ...original, tag_pictures: true, rate_quality: false });
    const status = await value("tag_model_status", {});
    const line = await $(".model-line");
    await line.waitForExist({ timeout: 10000 });

    if (!status.built_in) {
      await expect(line).toHaveText(expect.stringContaining("without the tagging model"));
    } else if (status.present) {
      await expect(line).toHaveText(expect.stringContaining("Tagging model ready"));
    } else {
      await expect(await $("button=Get the tagging model")).toExist();
    }
  });

  it("only offers judging once tagging is on, and its own model with it", async () => {
    await settle({ ...original, tag_pictures: true, rate_quality: true });
    const status = await value("quality_model_status", {});
    const lines = await $$(".model-line");
    expect(lines.length).toBe(2);

    if (!status.built_in) {
      await expect(lines[1]).toHaveText(expect.stringContaining("without the quality model"));
    } else if (status.present) {
      await expect(lines[1]).toHaveText(expect.stringContaining("Quality model ready"));
    } else {
      await expect(await $("button=Get the quality model")).toExist();
    }
  });

  it("puts the settings back the way it found them", async () => {
    await settle(original);
    const held = await value("get_settings", {});
    expect(held.tag_pictures).toBe(original.tag_pictures);
    expect(held.rate_quality).toBe(original.rate_quality);
  });
});
