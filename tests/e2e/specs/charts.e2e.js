describe("Charts view", () => {
  before(async () => {
    const charts = await $("button=Charts");
    await charts.click();
    await browser.waitUntil(async () => (await $$(".charts")).length > 0, {
      timeout: 60000,
      timeoutMsg: "the charts panel never mounted",
    });
  });

  it("either draws every figure or says there is nothing to draw", async () => {
    await expect(await $(".charts")).toExist();

    const empty = (await $$(".charts .placeholder")).length > 0;
    const figures = (await $$(".charts figure")).length;
    if (empty) {
      expect(figures).toBe(0);
    } else {
      expect(figures).toBe(5);
      expect((await $$(".charts .tile")).length).toBe(4);
      expect((await $$(".charts .cell")).length % 12).toBe(0);
      expect((await $$(".charts .column")).length).toBe(24);
    }
  });

  it("explains what each figure means rather than leaving it to the reader", async () => {
    const headings = await $$(".charts figure h3");
    if (headings.length === 0) return;
    for (const heading of headings) {
      await expect(heading).not.toHaveText("");
    }
    expect((await $$(".charts figcaption p")).length).toBe(headings.length);
  });

  it("goes back to the folder view", async () => {
    const folders = await $("button=Folders");
    await folders.click();
    await browser.waitUntil(async () => (await $$(".charts")).length === 0, {
      timeout: 5000,
      timeoutMsg: "the charts panel was never torn down",
    });
  });
});
