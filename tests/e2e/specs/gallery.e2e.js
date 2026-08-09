describe("Gallery view", () => {
  before(async () => {
    const gallery = await $("button=Gallery");
    await gallery.click();
    await browser.waitUntil(async () => (await $$(".loading")).length === 0, {
      timeout: 180000,
      timeoutMsg: "the plan never finished loading",
    });
    await browser.waitUntil(async () => (await $$(".gallery canvas")).length > 0, {
      timeout: 60000,
      timeoutMsg: "the gallery canvas never mounted",
    });
  });

  it("builds every shader program instead of falling back", async () => {
    await expect(await $(".gallery canvas")).toExist();
    expect((await $$(".gallery .fallback")).length).toBe(0);
  });

  it("draws the rooms without a WebGL error", async () => {
    const report = await browser.executeAsync(function (done) {
      var canvas = document.querySelector(".gallery canvas");
      var gl = canvas.getContext("webgl2");
      window.requestAnimationFrame(function () {
        window.requestAnimationFrame(function () {
          done({
            error: gl.getError(),
            program: gl.getParameter(gl.CURRENT_PROGRAM) !== null,
            width: canvas.width,
            height: canvas.height,
          });
        });
      });
    });

    expect(report.error).toBe(0);
    expect(report.program).toBe(true);
    expect(report.width).toBeGreaterThan(0);
  });

  it("offers the walk and names the room the visitor stands in", async () => {
    await expect(await $(".gallery .invite")).toExist();

    const plaque = await $(".gallery .plaque");
    await expect(plaque).toExist();
    await expect(plaque).not.toHaveText("");
  });

  it("hangs pictures on the walls rather than leaving them bare", async () => {
    const hung = await browser.executeAsync(function (done) {
      var text = document.querySelector(".gallery .invite").textContent || "";
      var match = text.match(/(\d+)\s+rooms/);
      var hungMatch = text.match(/(\d+)\s+hung/);
      done({
        rooms: match ? Number(match[1]) : 0,
        hung: hungMatch ? Number(hungMatch[1]) : 0,
      });
    });

    if (hung.rooms === 0) return;
    expect(hung.hung).toBeGreaterThan(0);
  });

  it("goes back to the folder view and tears the canvas down", async () => {
    const folders = await $("button=Folders");
    await folders.click();
    await browser.waitUntil(async () => (await $$(".gallery canvas")).length === 0, {
      timeout: 5000,
      timeoutMsg: "the gallery canvas was never torn down",
    });
  });
});
