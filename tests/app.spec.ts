// End to end: the built app, the real engine, a real browser. Each test
// starts with an empty browser, so the first thing on screen is the first
// demo, as it is for a person who opens Mjolnir for the first time.
import { expect, test, type Page } from "@playwright/test";

const tab = (page: Page, name: string) => page.locator(".bar .tab", { hasText: new RegExp(`^${name}$`) }).click();
const sentence = (page: Page) => page.locator(".sentence");
const hit = async (page: Page, pad: number) => {
  const p = page.locator(".pad").nth(pad);
  await p.dispatchEvent("pointerdown", { pointerId: 1 });
  await p.dispatchEvent("pointerup", { pointerId: 1 });
};

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/");
  await expect(page.locator(".shell")).toBeVisible();
  await expect(page.locator(".th .tname").first()).toBeVisible();
  (page as Page & { errors: string[] }).errors = errors;
});

test.afterEach(async ({ page }) => {
  expect((page as Page & { errors: string[] }).errors, "errors in the page").toEqual([]);
  // nothing ever scrolls the window sideways
  const [wide, inner] = await page.evaluate(() => [document.documentElement.scrollWidth, innerWidth]);
  expect(wide).toBeLessThanOrEqual(inner);
});

test("the first visit opens a finished piece", async ({ page }) => {
  await expect(page.locator(".th .tname")).toHaveText(["drums", "shadow sub", "shadow air", "soft piano", "dark bell"]);
  await expect(page.locator(".row")).toHaveCount(8);
  expect(await page.locator(".cell.filled").count()).toBeGreaterThan(8);
  await tab(page, "set");
  await expect(page.locator(".row.open .sname")).toHaveText("shadow walk");
});

test("a click plays a loop, another stops it, a double click steps inside", async ({ page }) => {
  const cell = page.locator(".cell.filled").first();
  await cell.click();
  await expect(sentence(page)).toContainText("playing");
  await expect(page.locator(".cell.live")).toHaveCount(1);
  // a second click sooner than this would be a double click
  await page.waitForTimeout(450);
  await cell.click();
  await expect(page.locator(".cell.live")).toHaveCount(0, { timeout: 8000 });
  await expect(page.locator(".loops")).toBeVisible();
  await page.waitForTimeout(450);
  await cell.dblclick();
  await expect(page.locator(".track")).toBeVisible();
  await page.locator(".back").click();
  await expect(page.locator(".loops")).toBeVisible();
});

test("a letter plays its row and the meters move", async ({ page }) => {
  await page.locator(".row").nth(2).click();
  await expect(page.locator(".cell.live")).toHaveCount(5);
  await expect
    .poll(() => page.locator(".th .meter").first().evaluate((e) => getComputedStyle(e).transform), { timeout: 8000 })
    .not.toBe("matrix(0, 0, 0, 1, 0, 0)");
  await page.locator(".status .primary").click();
  await expect(sentence(page)).not.toContainText("playing");
});

test("steps write a drum loop; a step shows its level, chance and repeats", async ({ page }) => {
  // row e of the drums is empty: one click steps in
  await page.locator(".cell").nth(4 * 5).click();
  await expect(page.locator(".track")).toBeVisible();
  await hit(page, 0);
  for (const s of [0, 4, 8, 12]) await page.locator(".step").nth(s).click();
  await expect(page.locator('.step[aria-pressed="true"]')).toHaveCount(4);
  // writing does not open a step's detail; holding it (or the right button) does, in the header's own line
  await expect(page.locator(".detail")).toHaveCount(0);
  const pads = (await page.locator(".pads").boundingBox())!;
  await page.locator(".step").nth(4).click({ button: "right" });
  await expect(page.locator(".detail .name")).toHaveText(["level", "chance", "repeat"]);
  expect((await page.locator(".pads").boundingBox())!.y).toBe(pads.y);
  await page.getByRole("button", { name: "done" }).click();
  await expect(page.locator('.step[aria-pressed="true"]')).toHaveCount(4);
  // undo takes the last step back
  await page.getByRole("button", { name: "undo" }).click();
  await expect(page.locator('.step[aria-pressed="true"]')).toHaveCount(3);
  await page.getByRole("button", { name: "redo" }).click();
  await expect(page.locator('.step[aria-pressed="true"]')).toHaveCount(4);
  // two bars: what was there repeats
  await page.locator(".play .mini", { hasText: /^2$/ }).first().click();
  await expect(page.getByText("showing bar")).toBeVisible();
});

test("the roll writes notes and chords; keys and pads play", async ({ page }) => {
  // the piano's row e is empty
  await page.locator(".cell").nth(4 * 5 + 3).click();
  await expect(page.locator(".sheet")).toBeVisible();
  const roll = (await page.locator(".roll").boundingBox())!;
  // the roll is what the screen is for
  expect(roll.height).toBeGreaterThan(page.viewportSize()!.height * 0.4);
  await page.mouse.click(roll.x + 80, roll.y + 30);
  await page.mouse.click(roll.x + roll.width * 0.6, roll.y + 70);
  await expect(page.locator(".roll .note")).toHaveCount(2);
  // chords are chosen from a sheet over the roll; choosing closes it
  await page.getByRole("button", { name: /^chords: off/ }).click();
  await page.getByRole("button", { name: "triad", exact: true }).click();
  await expect(page.locator(".veil")).toHaveCount(0);
  await page.mouse.click(roll.x + roll.width * 0.8, roll.y + 110);
  await expect(page.locator(".roll .note")).toHaveCount(5);
  // an open sheet shields the roll: a touch behind it closes it and writes nothing
  await page.getByRole("button", { name: /^note length/ }).click();
  await page.mouse.click(roll.x + roll.width * 0.3, roll.y + roll.height - 30);
  await expect(page.locator(".veil")).toHaveCount(0);
  await expect(page.locator(".roll .note")).toHaveCount(5);
  // what plays by hand folds in and out
  // (with chords on they are pads; on a wide screen they are already out)
  if (!(await page.locator(".melodic .pad").count())) await page.getByRole("button", { name: "pads", exact: true }).click();
  await expect(page.locator(".melodic .pad")).toHaveCount(16);
  await page.getByRole("button", { name: "keys", exact: true }).click();
  await expect(page.locator(".keys .white").first()).toBeVisible();
  await page.getByRole("button", { name: "keys", exact: true }).click();
  await expect(page.locator(".keys")).toHaveCount(0);
  // a note picked puts its detail in the bar, where nothing moves, and can be removed
  const before = (await page.locator(".roll").boundingBox())!;
  await page.locator(".roll .note").first().click();
  await expect(page.locator(".detail")).toBeVisible();
  expect((await page.locator(".roll").boundingBox())!.height).toBe(before.height);
  await page.locator(".detail").getByRole("button", { name: "remove" }).click();
  await expect(page.locator(".roll .note")).toHaveCount(4);
});

test("a held press or the right button on a loop offers to delete it", async ({ page, isMobile }) => {
  const sub = page.getByLabel(/^Shadow sub, section C/);
  await expect(sub).toHaveClass(/filled/);
  if (isMobile) {
    const b = (await sub.boundingBox())!;
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: b.x + 20, y: b.y + 20 }] });
    await page.waitForTimeout(650);
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  } else {
    await sub.click({ button: "right" });
  }
  await expect(page.getByRole("menu")).toBeVisible();
  // asking is not playing
  await expect(page.locator(".cell.live")).toHaveCount(0);
  // a touch elsewhere puts the menu away and changes nothing
  await page.mouse.click(5, 300);
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(sub).toHaveClass(/filled/);
  await sub.click({ button: "right" });
  await page.getByRole("menuitem", { name: "delete loop" }).click();
  await expect(sub).not.toHaveClass(/filled/);
  // and it comes back with undo
  await page.getByRole("button", { name: "undo" }).click();
  await expect(sub).toHaveClass(/filled/);
  // copy here, paste there
  await sub.click({ button: "right" });
  await page.getByRole("menuitem", { name: "copy", exact: true }).click();
  const empty = page.getByLabel(/^Shadow sub, section E/);
  await empty.click({ button: "right" });
  await page.getByRole("menuitem", { name: "paste" }).click();
  await expect(empty).toHaveClass(/filled/);
});

test("sounds are filed by kind and a macro sound opens every knob", async ({ page, isMobile }) => {
  await page.locator(".th").nth(4).click();
  if (isMobile) await page.getByRole("tab", { name: "sound" }).click();
  await expect(page.locator(".sound > .name")).toHaveText("dark bell");
  await expect(page.locator(".shelves button")).toHaveText(["bass", "keys", "pads", "leads", "yours"]);
  await expect(page.locator(".sound .group")).toHaveText(["tone", "envelope", "space"]);
  await page.locator(".shelves button", { hasText: "pads" }).click();
  await page.locator(".shelf button", { hasText: "night strings" }).click();
  await expect(page.locator(".sound > .name")).toHaveText("night strings");
  await page.locator(".shelves button", { hasText: "keys" }).click();
  await page.locator(".shelf button", { hasText: "soft piano" }).click();
  await expect(page.locator(".sound > .name")).toHaveText("soft piano");
});

test("an audio file becomes a sound of your own", async ({ page, isMobile }) => {
  await page.locator(".th").nth(4).click();
  if (isMobile) await page.getByRole("tab", { name: "sound" }).click();
  await page.locator(".shelves button", { hasText: "yours" }).click();
  // a second of a decaying 220 Hz tone, as a WAV
  const rate = 22050;
  const wav = Buffer.alloc(44 + rate * 2);
  wav.write("RIFF", 0);
  wav.writeUInt32LE(36 + rate * 2, 4);
  wav.write("WAVEfmt ", 8);
  wav.writeUInt32LE(16, 16);
  wav.writeUInt16LE(1, 20);
  wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(rate, 24);
  wav.writeUInt32LE(rate * 2, 28);
  wav.writeUInt16LE(2, 32);
  wav.writeUInt16LE(16, 34);
  wav.write("data", 36);
  wav.writeUInt32LE(rate * 2, 40);
  for (let i = 0; i < rate; i++) wav.writeInt16LE(Math.round(Math.sin((2 * Math.PI * 220 * i) / rate) * Math.exp(-i / rate) * 20000), 44 + i * 2);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "from a file" }).click()]);
  await chooser.setFiles({ name: "tone.wav", mimeType: "audio/wav", buffer: wav });
  await expect(page.locator(".sound > .name")).toHaveText("tone");
});

test("the mix has a strip per track and a master", async ({ page }) => {
  await tab(page, "mix");
  await expect(page.locator(".mix .strip")).toHaveCount(6);
  await page.locator(".mix .strip").first().getByRole("button", { name: "mute" }).click();
  await expect(page.locator(".mix .strip").first().getByRole("button", { name: "mute" })).toHaveAttribute("aria-pressed", "true");
});

test("the punch strip holds effects while a key is down", async ({ page }) => {
  await page.locator(".row").nth(2).click();
  await page.getByRole("button", { name: "fx", exact: true }).click();
  await expect(page.locator(".punch .key")).toHaveCount(8);
  const key = page.locator(".punch .key").nth(1);
  await key.dispatchEvent("pointerdown", { pointerId: 3 });
  await expect(key).toHaveClass(/held/);
  await key.dispatchEvent("pointerup", { pointerId: 3 });
  await expect(key).not.toHaveClass(/held/);
  await page.locator(".status .primary").click();
});

test("the song places, moves and exports", async ({ page }) => {
  await tab(page, "timeline");
  const before = await page.locator(".clip").count();
  expect(before).toBeGreaterThan(10);
  // the empty bars after the end of the song
  await page.locator(".scroll").evaluate((e) => (e.scrollLeft = e.scrollWidth));
  const view = (await page.locator(".scroll").boundingBox())!;
  const lane = (await page.locator(".lane").first().boundingBox())!;
  await page.mouse.click(view.x + view.width - 12, lane.y + 20);
  await expect(page.locator(".clip")).toHaveCount(before + 1);
  await expect(page.locator(".clip.picked")).toHaveCount(1);
  await page.getByRole("button", { name: "remove clip" }).click();
  await expect(page.locator(".clip")).toHaveCount(before);
  // export the mix and the midi: they arrive together as one zip
  await page.getByRole("button", { name: "export", exact: true }).click();
  await page.getByRole("button", { name: "midi" }).click();
  const [download] = await Promise.all([
    page.waitForEvent("download", { timeout: 60_000 }),
    page.locator(".sheet").getByRole("button", { name: "export", exact: true }).click(),
  ]);
  expect(download.suggestedFilename()).toBe("Shadow walk.zip");
});

test("sets are kept, copied, removed and survive a reload", async ({ page }) => {
  await tab(page, "set");
  await page.getByRole("button", { name: "new set" }).click();
  await expect(page.locator(".sets .row")).toHaveCount(2);
  await expect(page.locator(".row.open .sname")).toHaveText("untitled");
  await page.locator('input[aria-label="name"]').fill("Night one");
  await page.locator('input[aria-label="name"]').blur();
  await page.getByRole("button", { name: "fog" }).click();
  await expect(page.locator(".sets .row")).toHaveCount(3);
  await expect(page.locator(".row.open .sname")).toHaveText("fog");
  await expect(page.locator(".sets .sname", { hasText: "night one" })).toHaveCount(1);
  await page.reload();
  await expect(page.locator(".th .tname").first()).toBeVisible();
  await tab(page, "set");
  await expect(page.locator(".row.open .sname")).toHaveText("fog");
  await expect(page.locator(".sets .row")).toHaveCount(3);
  // remove asks once more
  await page.locator(".sets .row").last().getByRole("button", { name: /^remove/ }).click();
  await page.getByRole("button", { name: "remove for good" }).click();
  await expect(page.locator(".sets .row")).toHaveCount(2);
});

test("the help sheet opens on ? and closes on escape", async ({ page }) => {
  await page.keyboard.press("?");
  await expect(page.getByRole("dialog", { name: "how to play" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "how to play" })).toHaveCount(0);
});

test("a finger that scrolls the roll writes nothing; a tap still does", async ({ page, isMobile }) => {
  test.skip(!isMobile, "touch only");
  await page.locator(".cell").nth(4 * 5 + 3).click();
  await expect(page.locator(".sheet")).toBeVisible();
  const box = page.locator(".roll");
  const roll = (await box.boundingBox())!;
  const cdp = await page.context().newCDPSession(page);
  const x = roll.x + roll.width * 0.5;
  const drag = async (from: number, to: number) => {
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y: from }] });
    for (let i = 1; i <= 8; i++) {
      await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x, y: from + ((to - from) * i) / 8 }] });
      await page.waitForTimeout(16);
    }
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  };
  const before = await box.evaluate((el) => el.scrollTop);
  await drag(roll.y + roll.height - 20, roll.y + 20);
  await drag(roll.y + 20, roll.y + roll.height - 30);
  await page.waitForTimeout(300);
  // the sheet really moved under the finger
  expect(await box.evaluate((el) => el.scrollTop)).not.toBe(before);
  await drag(roll.y + roll.height - 20, roll.y + 30);
  await page.waitForTimeout(300);
  await expect(page.locator(".roll .note")).toHaveCount(0);
  await page.touchscreen.tap(x, roll.y + 40);
  await expect(page.locator(".roll .note")).toHaveCount(1);
});

test("a loop is carried to another octave or note, and doubled at an interval", async ({ page }) => {
  await page.locator(".cell").nth(4 * 5 + 3).click();
  await expect(page.locator(".sheet")).toBeVisible();
  const roll = (await page.locator(".roll").boundingBox())!;
  await page.mouse.click(roll.x + 80, roll.y + 30);
  await page.mouse.click(roll.x + roll.width * 0.6, roll.y + 70);
  const notes = page.locator(".roll .note");
  const names = () => notes.evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")!.split(" ")[0]).sort());
  await expect(notes).toHaveCount(2);
  const first = await names();
  // everything that changes the whole loop is behind one button, and stays open while it is used
  await page.getByRole("button", { name: "edit", exact: true }).click();
  await page.getByRole("button", { name: "octave lower" }).click();
  await expect.poll(names).toEqual(first.map((n) => n.replace(/-?\d+$/, (o) => String(Number(o) - 1))).sort());
  await page.getByRole("button", { name: "octave higher" }).click();
  await expect.poll(names).toEqual(first);
  await page.getByRole("button", { name: "note higher" }).click();
  await expect.poll(names).not.toEqual(first);
  await page.getByRole("button", { name: "note lower" }).click();
  await expect.poll(names).toEqual(first);
  await page.getByRole("button", { name: "3rd", exact: true }).click();
  await expect(notes).toHaveCount(4);
  // a third on the thirds stacks a triad; one undo takes a whole harmony away
  await page.getByRole("button", { name: "3rd", exact: true }).click();
  await expect(notes).toHaveCount(6);
  await page.getByRole("button", { name: "below" }).click();
  await page.getByRole("button", { name: "oct", exact: true }).click();
  await expect(notes).toHaveCount(12);
  await page.getByRole("button", { name: "undo" }).click();
  await expect(notes).toHaveCount(6);
  await page.keyboard.press("Escape");
  await expect(page.locator(".veil")).toHaveCount(0);
});

test("one loop of a playing row stops at the bar; a second touch stops it at once", async ({ page }) => {
  await page.getByLabel("section C: play the row").click();
  await expect(page.locator(".cell.live")).toHaveCount(5);
  const sub = page.getByLabel(/^Shadow sub, section C/);
  // told to stop: it says so while it plays out its bar, and the others go on
  await sub.click();
  await expect(sub).toHaveClass(/leaving/);
  await expect(sub).not.toHaveClass(/live/, { timeout: 8000 });
  await expect(page.locator(".cell.live")).toHaveCount(4);
  // a second touch does not wait for the bar, and does not bring it back
  const bell = page.getByLabel(/^Dark bell, section C/);
  await bell.click();
  await expect(bell).toHaveClass(/leaving/);
  await page.waitForTimeout(450);
  await bell.click();
  await expect(bell).not.toHaveClass(/live/, { timeout: 600 });
  await page.waitForTimeout(3500);
  await expect(page.locator(".cell.live")).toHaveCount(3);
});

test("an agent's messages write music the screen follows", async ({ page }) => {
  // the door a host opens for the Model Context Protocol
  const rpc = (method: string, params: unknown = {}) =>
    page.evaluate(async ([m, p]) => JSON.parse((await (window as unknown as { mjolnirMcp: (s: string) => Promise<string> }).mjolnirMcp(JSON.stringify({ jsonrpc: "2.0", id: 1, method: m, params: p })))!), [method, params] as const);
  const tool = async (name: string, args: unknown = {}) => {
    const r = (await rpc("tools/call", { name, arguments: args })).result;
    expect(r.isError, r.content[0].text).toBe(false);
    return r.content[0].text as string;
  };
  expect((await rpc("initialize", { protocolVersion: "2025-06-18" })).result.serverInfo.name).toBe("mjolnir");
  expect((await rpc("tools/list")).result.tools.length).toBeGreaterThan(15);
  await tool("new_set", { name: "From an agent", bpm: 90 });
  await expect(page.locator(".th .tname")).toHaveCount(0);
  await tool("add_track", { sound: "kit:shadow" });
  await tool("add_track", { sound: "macro:solina", name: "Strings" });
  await expect(page.locator(".th .tname")).toHaveText(["shadow", "strings"]);
  await tool("drum_groove", { track: 0, section: "A", groove: "boom_bap" });
  await tool("write_loop", { track: 1, section: "A", bars: 2, notes: [{ pitch: "A3", start: 0, length: 8 }, { pitch: "C4", start: 0, length: 8 }] });
  await expect(page.locator(".cell.filled")).toHaveCount(2);
  await tool("play", { section: "A" });
  await expect(page.locator(".cell.live")).toHaveCount(2);
  await expect(page.locator(".screen .br")).toContainText("90");
  await tool("stop");
  // and a person takes it back with the same undo
  await page.getByRole("button", { name: "undo" }).click();
  await expect(page.locator(".cell.filled")).toHaveCount(1);
});

test("a track is added from where the hand is, and opens on its sounds", async ({ page, isMobile }) => {
  await page.getByLabel("add a track").click();
  await page.getByRole("menuitem", { name: "keys" }).click();
  await expect(page.locator(".track")).toBeVisible();
  // a new track plays a sound that is on the shelves
  await expect(page.locator(".track [aria-label=tracks] .tab[aria-selected=true]")).toContainText("soft piano");
  if (isMobile) await expect(page.getByRole("tab", { name: "sound" })).toHaveAttribute("aria-selected", "true");
});

test("the tempo is changed on the screen; a row is copied down from its letter", async ({ page }) => {
  await page.getByLabel(/^tempo: 84 bpm/).click();
  await page.getByRole("button", { name: "faster by five" }).click();
  await page.getByRole("button", { name: "faster", exact: true }).click();
  await expect(page.getByLabel(/^tempo: 90 bpm/)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("group", { name: "tempo" })).toHaveCount(0);
  // row c has five loops; e is the next empty row
  await page.getByLabel("section C: play the row").click({ button: "right" });
  await expect(page.locator(".cell.live")).toHaveCount(0);
  await page.getByRole("menuitem", { name: "copy to the next empty row" }).click();
  await expect(page.locator(".cell.filled")).toHaveCount(13 + 5);
  await expect(page.getByLabel(/^Dark bell, section E/)).toHaveClass(/filled/);
  // one loop goes down on its own, to the next row where its track has none
  await page.getByLabel(/^Drums, section C/).click({ button: "right" });
  await page.getByRole("menuitem", { name: "copy to the next free row" }).click();
  await expect(page.getByLabel(/^Drums, section D/)).toHaveClass(/filled/);
});
