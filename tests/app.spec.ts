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
  await expect(page.locator(".detail")).toBeVisible();
  await expect(page.locator(".detail .name")).toHaveText(["level", "chance", "repeat"]);
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
  const roll = (await page.locator(".sheet").locator("..").boundingBox())!;
  await page.mouse.click(roll.x + 60, roll.y + 30);
  await page.mouse.click(roll.x + roll.width * 0.6, roll.y + 66);
  await expect(page.locator(".sheet .note")).toHaveCount(2);
  await page.getByRole("button", { name: "triad" }).click();
  await expect(page.locator(".melodic .pad")).toHaveCount(16);
  await page.mouse.click(roll.x + roll.width * 0.8, roll.y + 102);
  await expect(page.locator(".sheet .note")).toHaveCount(5);
  await page.getByRole("button", { name: "keys", exact: true }).click();
  await expect(page.locator(".keys .white").first()).toBeVisible();
  // a note picked shows its detail and can be removed
  await page.locator(".sheet .note").first().click();
  await expect(page.locator(".detail")).toBeVisible();
  await page.locator(".detail").getByRole("button", { name: "remove" }).click();
  await expect(page.locator(".sheet .note")).toHaveCount(4);
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
  await tab(page, "song");
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
