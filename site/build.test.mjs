import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { render, flatKeys, build, checkLinks } from "./build.mjs";

const ROOT = dirname(fileURLToPath(import.meta.url));
const json = (p) => JSON.parse(readFileSync(join(ROOT, p), "utf8"));

test("render replaces nested keys", () => {
  assert.equal(render("<p>{{a.b}}</p>", { a: { b: "hi" } }, "en"), "<p>hi</p>");
});

test("render fails loudly on a missing key", () => {
  assert.throws(() => render("{{a.c}}", { a: { b: "hi" } }, "pt"), /\[pt\] missing string: a\.c/);
});

test("en and pt carry exactly the same keys", () => {
  assert.deepEqual(flatKeys(json("src/i18n/pt.json")), flatKeys(json("src/i18n/en.json")));
});

test("build writes both languages and the 404 page", () => {
  const out = mkdtempSync(join(tmpdir(), "finoinc-site-"));
  build(out);
  assert.match(readFileSync(join(out, "index.html"), "utf8"), /<html lang="en"/);
  assert.match(readFileSync(join(out, "pt/index.html"), "utf8"), /<html lang="pt-BR"/);
  assert.ok(existsSync(join(out, "404.html")));
  assert.doesNotMatch(readFileSync(join(out, "pt/index.html"), "utf8"), /\{\{/);
});

test("every local link in the built site resolves", () => {
  const out = mkdtempSync(join(tmpdir(), "finoinc-site-"));
  build(out);
  assert.deepEqual(checkLinks(out), []);
});

test("the page requests nothing from third-party hosts", () => {
  const html = readFileSync(join(ROOT, "src/index.html"), "utf8");
  const loads = [...html.matchAll(/<(?:script|img|video|source|link)\b[^>]*?\s(?:src|href|poster)="(https?:[^"]+)"/g)]
    .filter((m) => !/rel="(?:alternate|canonical)"/.test(m[0]) && !/property="og:/.test(m[0]))
    .map((m) => m[1]);
  assert.deepEqual(loads, []);
});

test("hidden initial states only exist under .js/.motion, so content shows without JS", () => {
  const css = readFileSync(join(ROOT, "src/style.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const offenders = [...css.matchAll(/([^{}]+)\{[^}]*(?:opacity:\s*0[;\s}]|visibility:\s*hidden)/g)]
    .map((m) => m[1].trim())
    .filter((sel) => !sel.split(",").every((s) => /^\.(?:js|motion)\b/.test(s.trim())));
  assert.deepEqual(offenders, []);
});

test("every video can autoplay on Safari and falls back to a picture", () => {
  const html = readFileSync(join(ROOT, "src/index.html"), "utf8");
  const videos = [...html.matchAll(/<video\b[^>]*>/g)].map((m) => m[0]);
  assert.ok(videos.length >= 2);
  for (const v of videos) {
    assert.match(v, /\smuted\b/);
    assert.match(v, /\splaysinline\b/);
    assert.match(v, /\sposter="[^"]+"/);
    assert.match(v, /\spreload="none"/);
    assert.match(v, /\saria-hidden="true"/);
  }
});

test("the AI copy states the real tool defaults: read tools on, write tools off (README)", () => {
  assert.match(json("src/i18n/en.json").ai.body, /read tools start on, write tools start off/i);
  assert.match(json("src/i18n/pt.json").ai.body, /leitura começam ligadas e as de escrita, desligadas/i);
});

test("nothing focusable hides inside aria-hidden, and screenshot alt text is never display:none", () => {
  const html = readFileSync(join(ROOT, "src/index.html"), "utf8");
  for (const m of html.matchAll(/<(div|section|span)\b[^>]*aria-hidden="true"[^>]*>/g)) {
    // Walk to the matching close tag of the same element name.
    const tag = m[1];
    let depth = 0, i = m.index, end = html.length;
    const re = new RegExp(`<\\/?${tag}\\b[^>]*>`, "g");
    re.lastIndex = i;
    for (let t; (t = re.exec(html)); ) {
      depth += t[0].startsWith("</") ? -1 : 1;
      if (depth === 0) { end = re.lastIndex; break; }
    }
    assert.doesNotMatch(html.slice(i, end), /<(?:button|a|input|select|textarea)\b/, `focusable inside aria-hidden ${tag}`);
  }
  // On desktop the inline shots are hidden, so the stage images must carry the descriptions.
  const stageAlts = [...html.matchAll(/<img class="product__screen[^>]*\salt="([^"]*)"/g)].map((m) => m[1]);
  assert.equal(stageAlts.length, 5);
  for (const alt of stageAlts) assert.notEqual(alt, "");
});

// download.js is a classic script for the page; here it runs in a sandbox.
import { runInNewContext } from "node:vm";
function loadDownload() {
  const sandbox = { window: {} };
  runInNewContext(readFileSync(join(ROOT, "src/download.js"), "utf8"), sandbox);
  return sandbox.window.finoincDownload;
}

test("pick_package_per_os", () => {
  const { pickPackage, PACKAGES } = loadDownload();
  assert.equal(pickPackage("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15", "MacIntel", 0), "mac");
  assert.equal(pickPackage("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120", "Win32", 0), "windows");
  assert.equal(pickPackage("Mozilla/5.0 (X11; Linux x86_64) Firefox/120", "Linux x86_64", 0), "linux");
  assert.equal(PACKAGES.mac, "https://github.com/MateusFonseK/finoinc/releases/latest/download/Finoinc_universal.dmg");
  assert.equal(PACKAGES.windows, "https://github.com/MateusFonseK/finoinc/releases/latest/download/Finoinc_x64-setup.exe");
  assert.equal(PACKAGES.linux, "https://github.com/MateusFonseK/finoinc/releases/latest/download/Finoinc_amd64.AppImage");
});

test("phones_get_no_desktop_package", () => {
  const { pickPackage } = loadDownload();
  assert.equal(pickPackage("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)", "iPhone", 5), null);
  // iPadOS asks for desktop sites with a Mac user agent; only the touch screen tells.
  assert.equal(pickPackage("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15", "MacIntel", 5), null);
  assert.equal(pickPackage("Mozilla/5.0 (Linux; Android 14; Pixel 8) Chrome/120 Mobile", "Linux armv8l", 5), null);
  assert.equal(pickPackage("Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) Chrome/120", "Linux x86_64", 0), null);
  // Android's "desktop site" mode sends a plain X11 Linux user agent; the platform
  // still names the ARM CPU, and there is no ARM Linux package anyway.
  assert.equal(pickPackage("Mozilla/5.0 (X11; Linux x86_64) Chrome/120", "Linux armv8l", 5), null);
  assert.equal(pickPackage("Mozilla/5.0 (X11; Linux aarch64) Firefox/120", "Linux aarch64", 0), null);
});

test("the download button works without JavaScript and every OS is linked", () => {
  const out = mkdtempSync(join(tmpdir(), "finoinc-site-"));
  build(out);
  const html = readFileSync(join(out, "index.html"), "utf8");
  const cta = html.match(/<a\b[^>]*\sdata-download\b[^>]*>/)[0];
  assert.match(cta, /href="https:\/\/github\.com\/MateusFonseK\/finoinc\/releases\/latest"/);
  for (const file of ["Finoinc_universal.dmg", "Finoinc_x64-setup.exe", "Finoinc_amd64.AppImage", "Finoinc_amd64.deb"]) {
    assert.match(html, new RegExp(`href="https://github\\.com/MateusFonseK/finoinc/releases/latest/download/${file.replace(".", "\\.")}"`));
  }
  assert.doesNotMatch(html, /your Mac/i);
  // Tab roles only exist once the script builds the tabs; without it the panels are
  // plain sections and a screen reader hears no tabs that are not there.
  assert.doesNotMatch(html, /role="tabpanel"/);
});
