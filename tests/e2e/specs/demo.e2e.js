import { mkdirSync } from "node:fs";
import { join } from "node:path";

const FILM = process.env.EONSORT_FILM;
let frame = 0;

if (FILM) mkdirSync(FILM, { recursive: true });

const beat = async (ms = 900) => {
  if (!FILM) {
    await browser.pause(ms);
    return;
  }
  const until = Date.now() + ms;
  while (Date.now() < until) {
    await browser.saveScreenshot(
      join(FILM, `${String(frame++).padStart(5, "0")}.png`),
    );
  }
};

const settled = async (selector, timeout = 120000) => {
  await browser.waitUntil(
    async () => {
      const it = await $(selector);
      return it.isExisting();
    },
    { timeout, timeoutMsg: `${selector} never appeared` },
  );
};

describe("Demo", () => {
  it("sorts a folder of pictures", async () => {
    await browser.setWindowSize(1280, 820);
    await $("h1").waitForExist();
    await beat(Number(process.env.EONSORT_LEAD ?? 2200));

    const scan = await $("button=Scan");
    await scan.click();
    await settled(".tree");
    await beat(2600);

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
    await beat(1800);

    const files = await $$('.list [role="row"]');
    for (const at of [0, 1, 2]) {
      if (files[at]) {
        await files[at].click();
        await beat(2000);
      }
    }

    const gallery = await $("button=Gallery");
    if (process.env.EONSORT_GALLERY && (await gallery.isExisting())) {
      await gallery.click();
      await beat(4200);

      const invite = await $(".invite");
      if (await invite.isExisting()) {
        await invite.click();
        await beat(900);
      }

      const walking = await browser.execute(
        () => !!document.pointerLockElement,
      );
      console.log(`gallery: pointer lock ${walking ? "held" : "refused"}`);

      if (walking) {
        await browser.performActions([
          {
            type: "key",
            id: "walk",
            actions: [{ type: "keyDown", value: "w" }],
          },
        ]);
        await beat(7000);
        await browser.performActions([
          {
            type: "key",
            id: "walk",
            actions: [{ type: "keyUp", value: "w" }],
          },
        ]);
        await browser.releaseActions();
      } else {
        await beat(2600);
      }
    }

    for (const label of ["Charts", "Timeline"]) {
      const button = await $(`button=${label}`);
      if (!(await button.isExisting())) continue;
      await button.click();
      await beat(2800);
    }

    const back = await $("button=Folders");
    if (await back.isExisting()) {
      await back.click();
      await beat(1800);
    }
  });
});
