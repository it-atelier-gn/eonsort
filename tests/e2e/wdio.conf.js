import path from "path";
import { execFileSync, spawn, spawnSync } from "child_process";
import { fileURLToPath } from "url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");
const exeSuffix = process.platform === "win32" ? ".exe" : "";
const appPath = path.resolve(repoRoot, "target", "debug", `eonsort-desktop${exeSuffix}`);

let tauriDriver;
let exit = false;

export const config = {
  host: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.js"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application: appPath,
      },
    },
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },

  onPrepare: () => {
    spawnSync("npx", ["tauri", "build", "--debug", "--no-bundle"], {
      cwd: repoRoot,
      stdio: "inherit",
      shell: true,
    });
  },

  beforeSession: async () => {
    const driverArgs = [];
    if (process.platform === "win32") {
      const { download } = await import("edgedriver");
      const driverPath = await download(installedWebView2Version());
      driverArgs.push("--native-driver", driverPath);
    }

    tauriDriver = spawn("tauri-driver", driverArgs, {
      stdio: [null, process.stdout, process.stderr],
    });

    tauriDriver.on("error", (error) => {
      console.error("tauri-driver error:", error);
      process.exit(1);
    });
    tauriDriver.on("exit", (code) => {
      if (!exit) {
        console.error("tauri-driver exited with code:", code);
        process.exit(1);
      }
    });
  },

  before: async () => {
    await browser.waitUntil(async () => (await $$("[role=treeitem], .placeholder")).length > 0, {
      timeout: 180000,
      timeoutMsg: "the app never finished starting",
    });

    let quiet = 0;
    await browser.waitUntil(
      async () => {
        quiet = (await $$(".loading")).length === 0 ? quiet + 1 : 0;
        return quiet >= 5;
      },
      { timeout: 180000, interval: 300, timeoutMsg: "the plan never settled" },
    );
  },

  afterSession: () => {
    closeTauriDriver();
  },
};

function installedWebView2Version() {
  const key =
    "HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
  const output = execFileSync("reg", ["query", key, "/v", "pv"], { encoding: "utf8" });
  const match = output.match(/pv\s+REG_SZ\s+(\S+)/);
  if (!match) {
    throw new Error("could not determine the installed WebView2 Runtime version");
  }
  return match[1];
}

function closeTauriDriver() {
  exit = true;
  tauriDriver?.kill();
}

function onShutdown(fn) {
  const cleanup = () => {
    try {
      fn();
    } finally {
      process.exit();
    }
  };

  process.on("exit", cleanup);
  process.on("SIGINT", cleanup);
  process.on("SIGTERM", cleanup);
  process.on("SIGHUP", cleanup);
  process.on("SIGBREAK", cleanup);
}

onShutdown(() => {
  closeTauriDriver();
});
