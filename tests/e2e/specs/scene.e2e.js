describe("Scene view", () => {
  let photos = 0;

  async function excuse() {
    const said = await $$(".scene .prompt");
    if (said.length === 0) return "";
    const text = (await said[0].getText()).replace(/\s+/g, " ").trim();
    return text.startsWith("That picture would not open") ? text : "";
  }

  before(async () => {
    const scene = await $("button=Scene");
    await scene.click();
    await browser.waitUntil(async () => (await $$(".loading")).length === 0, {
      timeout: 180000,
      timeoutMsg: "the plan never finished loading",
    });
    await browser.waitUntil(async () => (await $$(".scene canvas")).length > 0, {
      timeout: 60000,
      timeoutMsg: "the scene canvas never mounted",
    });

    try {
      await browser.waitUntil(async () => (await $$(".scene .strip .shot")).length > 0, {
        timeout: 10000,
      });
      photos = (await $$(".scene .strip .shot")).length;
    } catch {
      photos = 0;
    }
    console.log(`the open plan offers ${photos} photographs to the scene view`);
  });

  it("builds the shader program instead of falling back", async () => {
    await expect(await $(".scene canvas")).toExist();
    expect((await $$(".scene .fallback")).length).toBe(0);
  });

  it("draws without a WebGL error", async () => {
    const report = await browser.executeAsync(function (done) {
      var canvas = document.querySelector(".scene canvas");
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
    expect(report.width).toBeGreaterThan(0);
    expect(report.height).toBeGreaterThan(0);
  });

  it("shows the filmstrip so a picture can be chosen", async () => {
    await expect(await $(".scene .strip")).toExist();
  });

  it("offers the fit handles before it offers the walk", async function () {
    if (photos === 0) this.skip();

    const shots = await $$(".scene .strip .shot");
    await shots[0].click();
    await browser.waitUntil(async () => (await $$(".scene .shot.on")).length === 1, {
      timeout: 10000,
      timeoutMsg: "clicking a thumbnail did not choose it",
    });
    await browser.waitUntil(async () => (await $$(".scene .frame")).length > 0 || (await excuse()), {
      timeout: 35000,
      timeoutMsg: "the picture neither opened nor said why",
    });

    const said = await excuse();
    expect(said).toBe("");
    expect((await $$(".scene .frame")).length).toBe(1);

    expect((await $$(".scene .handle")).length).toBe(5);
    expect((await $$(".scene .handle.vp")).length).toBe(1);
    await expect(await $(".scene .plaque")).toExist();
  });

  it("rebuilds the room when the back wall is dragged in", async function () {
    if (photos === 0 || (await $$(".scene .frame")).length === 0) this.skip();

    const plaque = await $(".scene .plaque");
    const before = Number(await plaque.getAttribute("data-depth"));

    const corner = (await $$(".scene .handle.corner"))[3];
    const box = await corner.getLocation();
    const size = await corner.getSize();

    await browser
      .action("pointer")
      .move({ x: Math.round(box.x + size.width / 2), y: Math.round(box.y + size.height / 2) })
      .down()
      .move({ x: Math.round(box.x + size.width / 2) + 6, y: Math.round(box.y + size.height / 2) - 30 })
      .up()
      .perform();

    await browser.waitUntil(
      async () => Number(await plaque.getAttribute("data-depth")) !== before,
      { timeout: 5000, timeoutMsg: "dragging the back wall did not rebuild the room" },
    );

    expect(Number(await plaque.getAttribute("data-depth"))).toBeGreaterThan(before);
  });

  it("goes back to the folder view and tears the canvas down", async () => {
    const folders = await $("button=Folders");
    await folders.click();
    await browser.waitUntil(async () => (await $$(".scene canvas")).length === 0, {
      timeout: 5000,
      timeoutMsg: "the scene canvas was never torn down",
    });
  });
});
