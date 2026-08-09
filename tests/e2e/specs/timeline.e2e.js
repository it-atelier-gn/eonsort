describe("Timeline view", () => {
  before(async () => {
    const timeline = await $("button=Timeline");
    await timeline.click();
    await browser.waitUntil(async () => (await $$(".loading")).length === 0, {
      timeout: 180000,
      timeoutMsg: "the plan never finished loading",
    });
    await browser.waitUntil(async () => (await $$("canvas")).length > 0, {
      timeout: 60000,
      timeoutMsg: "the timeline canvas never mounted",
    });
  });

  it("builds every shader program instead of falling back", async () => {
    await expect(await $("canvas")).toExist();
    expect((await $$(".fallback")).length).toBe(0);
  });

  it("holds a live WebGL2 context that reports no error", async () => {
    const report = await browser.execute(function () {
      var canvas = document.querySelector("canvas");
      if (!canvas) return { error: "no canvas" };
      var gl = canvas.getContext("webgl2");
      if (!gl) return { error: "no webgl2 context" };
      return {
        error: null,
        glError: gl.getError(),
        width: canvas.width,
        height: canvas.height,
        version: gl.getParameter(gl.VERSION),
      };
    });

    expect(report.error).toBe(null);
    expect(report.glError).toBe(0);
    expect(report.width).toBeGreaterThan(0);
    expect(report.height).toBeGreaterThan(0);
    expect(report.version).toContain("WebGL");
  });

  it("runs its draw calls rather than sitting idle", async () => {
    const drawn = await browser.executeAsync(function (done) {
      var gl = document.querySelector("canvas").getContext("webgl2");
      window.requestAnimationFrame(function () {
        window.requestAnimationFrame(function () {
          done({
            program: gl.getParameter(gl.CURRENT_PROGRAM) !== null,
            buffer: gl.getParameter(gl.ARRAY_BUFFER_BINDING) !== null,
            blending: gl.getParameter(gl.BLEND),
            error: gl.getError(),
          });
        });
      });
    });

    expect(drawn.error).toBe(0);
    expect(drawn.program).toBe(true);
    expect(drawn.buffer).toBe(true);
    expect(drawn.blending).toBe(true);
  });

  it("offers all three layouts and survives switching between them", async () => {
    for (const label of ["Time helix", "Memory terrain", "Disagreement field"]) {
      const button = await $(`button=${label}`);
      await expect(button).toExist();
      await button.click();
      await browser.pause(120);
    }

    const glError = await browser.execute(function () {
      var canvas = document.querySelector("canvas");
      return canvas.getContext("webgl2").getError();
    });
    expect(glError).toBe(0);
  });

  it("turns points into media once the camera is close enough", async () => {
    const canvas = await $("canvas");
    for (let i = 0; i < 12; i += 1) {
      await browser.execute(function (element) {
        element.dispatchEvent(
          new WheelEvent("wheel", { deltaY: -140, bubbles: true, cancelable: true }),
        );
      }, canvas);
    }

    await browser.waitUntil(async () => (await $$(".tiles")).length > 0, {
      timeout: 5000,
      timeoutMsg: "the media overlay never mounted",
    });

    const populated = await browser.waitUntil(
      async () => (await $$(".tiles .tile")).length > 0,
      { timeout: 20000, timeoutMsg: "zooming in never turned any point into its picture" },
    );
    expect(populated).toBeTruthy();

    for (const tile of await $$(".tiles .tile")) {
      await expect(await tile.$("img, video, .film")).toExist();
    }

    const glError = await browser.execute(function () {
      return document.querySelector("canvas").getContext("webgl2").getError();
    });
    expect(glError).toBe(0);
  });

  it("goes back to the folder view and tears the canvas down", async () => {
    const folders = await $("button=Folders");
    await folders.click();
    await browser.waitUntil(async () => (await $$("canvas")).length === 0, {
      timeout: 5000,
      timeoutMsg: "the timeline canvas was never torn down",
    });
  });
});
