import { mkdirSync } from "node:fs";
import { join } from "node:path";

const SHOTS = process.env.EONSORT_SHOTS ?? "shots";
const WIDE = { width: 1600, height: 1000 };

const shot = async (name) => {
  mkdirSync(SHOTS, { recursive: true });
  await browser.pause(600);
  await browser.saveScreenshot(join(SHOTS, `${name}.png`));
};

const settled = async (selector, timeout = 180000) => {
  await browser.waitUntil(
    async () => {
      const it = await $(selector);
      return it.isExisting();
    },
    { timeout, timeoutMsg: `${selector} never appeared` },
  );
};

describe("Screenshots", () => {
  before(async () => {
    await browser.setWindowSize(WIDE.width, WIDE.height);
    await $("h1").waitForExist();
  });

  it("shows the window before anything has been asked of it", async () => {
    await shot("01-before-a-scan");
  });

  const quiet = async () => {
    await browser.waitUntil(
      async () => !(await $("*=Tagging pictures").isExisting()),
      { timeout: 300000, timeoutMsg: "tagging never finished" },
    );
  };

  it("scans the sample pictures", async () => {
    const scan = await $("button=Scan");
    await scan.click();
    await settled(".tree");
    await quiet();
    await browser.pause(1500);
    await shot("02-the-plan");
  });

  it("opens the fullest folder and a picture in it", async () => {
    const rows = await $$('.tree [role="treeitem"]');
    let best = null;
    let most = 0;
    for (const row of rows) {
      const said = (await row.getText()).replace(/\s+/g, " ");
      const count = Number(said.match(/ (\d+) /)?.[1] ?? 0);
      if (count > most) {
        most = count;
        best = row;
      }
    }
    if (best) await best.click();
    await browser.pause(1500);
    await shot("03-a-folder");

    const files = await $$('.list [role="row"]');
    if (files.length > 0) {
      await files[Math.min(1, files.length - 1)].click();
      await browser.pause(3000);
    }
    await shot("04-a-picture");
  });

  it("walks the other views", async () => {
    for (const [name, label] of [
      ["05-charts", "Charts"],
      ["06-gallery", "Gallery"],
      ["07-rings", "Rings"],
    ]) {
      const button = await $(`button=${label}`);
      if (!(await button.isExisting())) continue;
      await button.click();
      await browser.pause(2200);
      await shot(name);
    }
  });

  it("comes back to the folders", async () => {
    const folders = await $("button=Folders");
    if (await folders.isExisting()) {
      await folders.click();
      await browser.pause(1000);
    }
    await shot("08-back-to-the-folders");
  });
});
