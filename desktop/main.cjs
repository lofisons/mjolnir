// Mjolnir on a desktop: a window around the same app that runs in a browser.
// The engine is the same WebAssembly module, in the same Chromium, so what is
// heard here is what is heard everywhere else.
const { app, BrowserWindow, Menu, protocol, net, shell, session, clipboard, dialog } = require("electron");
const path = require("node:path");
const fs = require("node:fs");
const http = require("node:http");
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

// ---- an agent at the instrument
//
// With "Let an agent play" on, the app listens on this computer only for the
// Model Context Protocol (streamable HTTP, at /mcp). Each message goes to the
// same engine the window plays, so what an agent writes appears and sounds
// here at once. Off unless the person turns it on; `--mcp` turns it on for
// one run.
const AGENT_PORT = 47800;
const agentFile = () => path.join(app.getPath("userData"), "agent.json");
let agent = { on: false, port: AGENT_PORT };
let agentServer = null;
let agentPort = 0;

function loadAgent() {
  try {
    agent = { ...agent, ...JSON.parse(fs.readFileSync(agentFile(), "utf8")) };
  } catch {
    /* never set */
  }
  if (process.argv.includes("--mcp")) agent.on = true;
}
const agentUrl = () => `http://127.0.0.1:${agentPort}/mcp`;

function answer(res, status, body, headers = {}) {
  res.writeHead(status, { "Cache-Control": "no-store", ...headers });
  res.end(body);
}

function serveAgent(req, res) {
  // this computer only, and never a web page: a browser always says where it comes from
  const host = (req.headers.host || "").toLowerCase();
  if (host !== `127.0.0.1:${agentPort}` && host !== `localhost:${agentPort}`) return answer(res, 403, "not this address");
  if (req.headers.origin) return answer(res, 403, "not from a web page");
  if (new URL(req.url, "http://x").pathname !== "/mcp") return answer(res, 404, "the door is /mcp");
  if (req.method !== "POST") return answer(res, 405, "POST a message", { Allow: "POST" });
  const chunks = [];
  let size = 0;
  req.on("data", (c) => {
    size += c.length;
    if (size > 4 << 20) req.destroy();
    else chunks.push(c);
  });
  req.on("end", async () => {
    const text = Buffer.concat(chunks).toString("utf8");
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      return answer(res, 400, JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "not JSON" } }), { "Content-Type": "application/json" });
    }
    if (!win || win.webContents.isLoading()) return answer(res, 503, "the instrument is not open yet");
    try {
      const reply = await win.webContents.executeJavaScript(`window.mjolnirMcp(${JSON.stringify(JSON.stringify(message))})`);
      // nothing to say back: a notification
      if (reply === null || reply === undefined) return answer(res, 202, "");
      answer(res, 200, reply, { "Content-Type": "application/json" });
    } catch (e) {
      answer(res, 500, JSON.stringify({ jsonrpc: "2.0", id: message.id ?? null, error: { code: -32603, message: String(e) } }), { "Content-Type": "application/json" });
    }
  });
}

function listenAgent(port, tries) {
  const server = http.createServer(serveAgent);
  server.once("error", (e) => {
    if (e.code === "EADDRINUSE" && tries > 0) return listenAgent(port + 1, tries - 1);
    agentServer = null;
    agent.on = false;
    Menu.setApplicationMenu(menu());
    dialog.showErrorBox("Mjolnir", `An agent could not be let in: ${e.message}`);
  });
  server.listen(port, "127.0.0.1", () => {
    agentServer = server;
    agentPort = port;
    Menu.setApplicationMenu(menu());
  });
}

function setAgent(on) {
  agent.on = on;
  try {
    fs.writeFileSync(agentFile(), JSON.stringify({ on, port: agent.port }));
  } catch {
    /* the choice lasts for this run */
  }
  if (on && !agentServer) listenAgent(agent.port, 10);
  if (!on && agentServer) {
    agentServer.close();
    agentServer.closeAllConnections?.();
    agentServer = null;
    Menu.setApplicationMenu(menu());
  }
}

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
    {
      label: "Agent",
      submenu: [
        { label: "Let an agent play (MCP)", type: "checkbox", checked: agent.on, click: (item) => setAgent(item.checked) },
        { label: agentServer ? `Listening at ${agentUrl()}` : "Not listening", enabled: false },
        { label: "Copy its address", enabled: !!agentServer, click: () => clipboard.writeText(agentUrl()) },
        { type: "separator" },
        { label: "How to connect an agent", click: () => shell.openExternal("https://github.com/lofisons/mjolnir/blob/main/guide/AGENTS.md") },
      ],
    },
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
    loadAgent();
    Menu.setApplicationMenu(menu());
    pending = process.argv.slice(1).find(isSet) ?? pending;
    open();
    if (agent.on) listenAgent(agent.port, 10);
    app.on("activate", () => BrowserWindow.getAllWindows().length === 0 && open());
  });
  app.on("window-all-closed", () => process.platform !== "darwin" && app.quit());
}
