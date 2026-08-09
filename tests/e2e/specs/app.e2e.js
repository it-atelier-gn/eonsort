describe("Eonsort", () => {
  it("shows the app title", async () => {
    const title = await $("h1");
    await expect(title).toHaveText("Eonsort");
  });

  it("offers the main actions", async () => {
    const scan = await $("button=Scan");
    const copyFiles = await $("button=Copy files");
    const checkResult = await $("button=Check result");
    await expect(scan).toExist();
    await expect(copyFiles).toExist();
    await expect(checkResult).toExist();
  });

  it("reports a status in the footer", async () => {
    const status = await $("footer .status");
    await expect(status).not.toHaveText("");
  });
});
