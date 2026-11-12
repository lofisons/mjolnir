// Mjolnir on a desktop: a window around the same app that runs in a browser.
// The engine is the same WebAssembly module, in the same Chromium, so what is
// heard here is what is heard everywhere else.
const { app, BrowserWindow, Menu, protocol, net, shell, session } = require("electron");
const path = require("node:path");
const fs = require("node:fs");
const { pathToFileURL } = require("node:url");

const ROOT = path.join(__dirname, "app");
// its own scheme, treated like https: storage, workers and audio worklets all work
protocol.registerSchemesAsPrivileged([
  { scheme: "app", privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true, codeCache: true } },
]);
// sound may start without waiting for a click
app.commandLine.appendSwitch("autoplay-policy", "no-user-gesture-required");

let win = null;
/** A set handed over by the system (double click, "open with"), waiting for the window. */
let pending = null;

const isSet = (p) => /\.(mjolnir|yggdrasil)$/i.test(p || "");

function hand(file) {
  if (!file) return;
  if (!win || win.webContents.isLoading()) {
    pending = file;
    return;
  }
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch {
    return;
  }
  win.webContents.executeJavaScript(
    `window.dispatchEvent(new CustomEvent("mjolnir-open", { detail: ${JSON.stringify(text)} }))`,
  );
}

const tell = (what) =>
  win && win.webContents.executeJavaScript(`window.dispatchEvent(new CustomEvent("mjolnir-menu", { detail: ${JSON.stringify(what)} }))`);

function menu() {
  const mac = process.platform === "darwin";
  return Menu.buildFromTemplate([
    ...(mac ? [{ role: "appMenu" }] : []),
    {
      label: "File",
      submenu: [
        { label: "New set", accelerator: "CmdOrCtrl+N", click: () => tell("new") },
        { label: "Open a file…", accelerator: "CmdOrCtrl+O", click: () => tell("open") },
        { label: "Save this set to a file…", accelerator: "CmdOrCtrl+S", click: () => tell("save") },
        { type: "separator" },
        { label: "Export audio…", accelerator: "CmdOrCtrl+E", click: () => tell("export") },
        { type: "separator" },
        mac ? { role: "close" } : { role: "quit" },
      ],
    },
    {
      label: "Edit",
      submenu: [
        { label: "Undo", accelerator: "CmdOrCtrl+Z", click: () => tell("undo") },
        { label: "Redo", accelerator: "CmdOrCtrl+Shift+Z", click: () => tell("redo") },
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
      ],
    },
    { label: "View", submenu: [{ role: "togglefullscreen" }, { role: "resetZoom" }, { role: "zoomIn" }, { role: "zoomOut" }] },
    { role: "windowMenu" },
    {
      label: "Help",
      submenu: [
        { label: "How to play", accelerator: "F1", click: () => tell("help") },
        { label: "Mjolnir on GitHub", click: () => shell.openExternal("https://github.com/lofisons/mjolnir") },
      ],
    },
  ]);
}

function open() {
  win = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 380,
    minHeight: 620,
    backgroundColor: "#0b0b0a",
    title: "Mjolnir",
    icon: path.join(__dirname, "build", "icon.png"),
    autoHideMenuBar: true,
    webPreferences: { contextIsolation: true, sandbox: true, backgroundThrottling: false },
  });
  win.loadURL("app://mjolnir/index.html");
  win.webContents.on("did-finish-load", () => {
    if (pending) {
      const f = pending;
      pending = null;
      // the app needs a moment to wake its engine
      setTimeout(() => hand(f), 600);
    }
  });
  // links leave to the browser; nothing else opens a window
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:/.test(url)) shell.openExternal(url);
    return { action: "deny" };
  });
  win.on("closed", () => (win = null));
}

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on("second-instance", (_e, argv) => {
    if (win) {
      if (win.isMinimized()) win.restore();
      win.focus();
    }
    hand(argv.find(isSet));
  });
  // macOS hands files over as an event, not as arguments
  app.on("open-file", (e, file) => {
    e.preventDefault();
    hand(file);
  });
  app.whenReady().then(() => {
    protocol.handle("app", (request) => {
      const { pathname } = new URL(request.url);
      const file = path.normalize(path.join(ROOT, decodeURIComponent(pathname)));
      // nothing outside the app's own folder
      if (!file.startsWith(ROOT)) return new Response("no", { status: 403 });
      return net.fetch(pathToFileURL(file).toString());
    });
    // the microphone (to sample) and MIDI controllers; nothing else
    session.defaultSession.setPermissionRequestHandler((_wc, permission, allow) =>
      allow(["media", "midi", "midiSysex", "fullscreen"].includes(permission)),
    );
    session.defaultSession.setPermissionCheckHandler((_wc, permission) =>
      ["media", "midi", "midiSysex", "fullscreen"].includes(permission),
    );
    Menu.setApplicationMenu(menu());
    pending = process.argv.slice(1).find(isSet) ?? pending;
    open();
    app.on("activate", () => BrowserWindow.getAllWindows().length === 0 && open());
  });
  app.on("window-all-closed", () => process.platform !== "darwin" && app.quit());
}
