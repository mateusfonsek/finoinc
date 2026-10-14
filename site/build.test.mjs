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
  assert.ok(videos.length >= 3);
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
