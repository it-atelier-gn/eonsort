const UNREACHABLE = "http://127.0.0.1:1";

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

async function rejection(command, args) {
  try {
    const result = await call(command, args);
    return result.ok ? null : result.error;
  } catch (error) {
    return String(error.message || error);
  }
}

describe("Model download and removal", () => {
  let original = null;

  before(async () => {
    original = (await call("get_settings", {})).value;
  });

  after(async () => {
    if (original) {
      await call("save_settings", { settings: original });
      await browser.refresh();
    }
  });

  it("refuses to manage models on an OpenAI-compatible runner", async () => {
    const message = await rejection("uninstall_model", {
      config: { ...original.ai, enabled: true, api: "open_ai", endpoint: UNREACHABLE },
      model: "qwen2.5vl",
    });

    expect(message).toContain("Ollama");
  });

  it("refuses to manage models while the local model is switched off", async () => {
    const message = await rejection("uninstall_model", {
      config: { ...original.ai, enabled: false, api: "ollama" },
      model: "qwen2.5vl",
    });

    expect(message).toMatch(/switched off/i);
  });

  describe("with a runner configured but not running", () => {
    before(async () => {
      await call("save_settings", {
        settings: {
          ...original,
          ai: { ...original.ai, enabled: true, api: "ollama", endpoint: UNREACHABLE },
        },
      });
      await browser.refresh();
      await (await $("#ai-enabled")).waitForExist({ timeout: 10000 });
    });

    it("offers a download for each model it cannot find", async () => {
      const download = await $$("button=Download");
      expect(download.length).toBe(2);
      await expect(download[0]).toBeEnabled();
    });

    it("says the runner is unreachable rather than silently doing nothing", async () => {
      const download = await $$("button=Download");
      await download[0].click();

      const failure = await $(".error");
      await failure.waitForExist({ timeout: 25000 });
      await expect(failure).toHaveText(expect.stringContaining("127.0.0.1:1"));
    });
  });
});
