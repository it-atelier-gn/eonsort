const PREVIEW_BAR = 'button[aria-label="Resize the preview"]';

const boxOf = (selector) =>
  browser.execute((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return {
      x: Math.round(r.x + r.width / 2),
      y: Math.round(r.y + r.height / 2),
      width: Math.round(r.width),
      height: Math.round(r.height),
    };
  }, selector);

const dragBy = async (selector, byX, byY) => {
  const at = await boxOf(selector);
  if (!at) throw new Error(`no ${selector} to drag`);

  await browser.performActions([
    {
      type: "pointer",
      id: "finger",
      parameters: { pointerType: "mouse" },
      actions: [
        { type: "pointerMove", duration: 0, x: at.x, y: at.y },
        { type: "pointerDown", button: 0 },
        { type: "pause", duration: 50 },
        { type: "pointerMove", duration: 120, x: at.x + byX, y: at.y + byY },
        { type: "pause", duration: 50 },
        { type: "pointerUp", button: 0 },
      ],
    },
  ]);
  await browser.releaseActions();
};

const widthOf = async (selector) => (await boxOf(selector)).width;

const reset = async () => {
  await browser.execute(() => {
    localStorage.removeItem("eonsort.preview.pane");
  });
  await browser.refresh();
  await $("h1").waitForExist();
};

describe("The dividers", () => {
  beforeEach(reset);
  after(reset);

  it("sits between the file list and the preview", async () => {
    const bar = await boxOf(PREVIEW_BAR);
    const pane = await boxOf("aside.pane");

    expect(bar).not.toBeNull();
    expect(bar.x).toBeLessThan(pane.x);
  });

  it("leaves every part of the window on one row", async () => {
    const tops = await browser.execute(() =>
      Array.from(document.querySelector("main").children).map((c) =>
        Math.round(c.getBoundingClientRect().top),
      ),
    );

    expect(new Set(tops).size).toBe(1);
  });

  it("widens the preview when dragged to the left", async () => {
    const before = await widthOf("aside.pane");
    await dragBy(PREVIEW_BAR, -90, 0);
    const after = await widthOf("aside.pane");

    expect(after).toBeGreaterThan(before + 50);
  });

  it("narrows the preview when dragged to the right", async () => {
    await dragBy(PREVIEW_BAR, -120, 0);
    const wide = await widthOf("aside.pane");

    await dragBy(PREVIEW_BAR, 60, 0);
    const narrow = await widthOf("aside.pane");

    expect(narrow).toBeLessThan(wide);
  });

  it("never lets the preview shrink away entirely", async () => {
    const at = await boxOf(PREVIEW_BAR);
    const edge = await browser.execute(() => window.innerWidth - 2);

    await browser.performActions([
      {
        type: "pointer",
        id: "finger",
        parameters: { pointerType: "mouse" },
        actions: [
          { type: "pointerMove", duration: 0, x: at.x, y: at.y },
          { type: "pointerDown", button: 0 },
          { type: "pointerMove", duration: 150, x: edge, y: at.y },
          { type: "pointerUp", button: 0 },
        ],
      },
    ]);
    await browser.releaseActions();

    expect(await widthOf("aside.pane")).toBeGreaterThanOrEqual(230);
  });

  it("remembers the width across a reload", async () => {
    await dragBy(PREVIEW_BAR, -80, 0);
    const before = await widthOf("aside.pane");

    await browser.refresh();
    await $("h1").waitForExist();

    expect(
      Math.abs((await widthOf("aside.pane")) - before),
    ).toBeLessThanOrEqual(2);
  });

  it("moves with the arrow keys once it has focus", async () => {
    const before = await widthOf("aside.pane");
    await browser.execute(
      (sel) => document.querySelector(sel).focus(),
      PREVIEW_BAR,
    );
    await browser.keys(["ArrowLeft", "ArrowLeft"]);

    expect(await widthOf("aside.pane")).toBeGreaterThan(before);
  });

  it("goes back to where it started on a double click", async () => {
    await dragBy(PREVIEW_BAR, -120, 0);
    expect(await widthOf("aside.pane")).toBeGreaterThan(340);

    const bar = await $(PREVIEW_BAR);
    await bar.doubleClick();

    expect(Math.abs((await widthOf("aside.pane")) - 340)).toBeLessThanOrEqual(
      2,
    );
  });
});
