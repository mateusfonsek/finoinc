import { readFileSync, writeFileSync, mkdirSync, cpSync, rmSync, existsSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(fileURLToPath(import.meta.url));
const SITE_URL = "https://mateusfonsek.github.io/finoinc/";

const LOCALES = {
  en: { out: "index.html", vars: { base: "./", lang: "en", screens: "en", alt_href: "pt/", alt_label: "PT", site_url: SITE_URL } },
  pt: { out: "pt/index.html", vars: { base: "../", lang: "pt-BR", screens: "pt", alt_href: "../", alt_label: "EN", site_url: SITE_URL } },
};

export function render(template, strings, locale) {
  return template.replace(/\{\{\s*([\w.]+)\s*\}\}/g, (_, key) => {
    const value = key.split(".").reduce((node, part) => node?.[part], strings);
    if (typeof value !== "string") throw new Error(`[${locale}] missing string: ${key}`);
    return value;
  });
}

export function flatKeys(obj, prefix = "") {
  return Object.entries(obj)
    .flatMap(([k, v]) => (typeof v === "string" ? [prefix + k] : flatKeys(v, `${prefix}${k}.`)))
    .sort();
}

const read = (p) => readFileSync(join(ROOT, p), "utf8");

export function build(outDir = join(ROOT, "_site")) {
  rmSync(outDir, { recursive: true, force: true });
  const page = read("src/index.html");
  const strings = {};
  for (const [locale, { out, vars }] of Object.entries(LOCALES)) {
    strings[locale] = { ...JSON.parse(read(`src/i18n/${locale}.json`)), ...vars };
    const target = join(outDir, out);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, render(page, strings[locale], locale));
  }
  // One 404 for every path: GitHub Pages serves it from any depth, so it carries both
  // languages instead of guessing one.
  const notFound = { en: strings.en, pt: strings.pt, site_url: SITE_URL };
  writeFileSync(join(outDir, "404.html"), render(read("src/404.html"), notFound, "404"));
  for (const dir of ["assets", "vendor"]) cpSync(join(ROOT, dir), join(outDir, dir), { recursive: true });
  for (const file of ["style.css", "main.js"]) cpSync(join(ROOT, "src", file), join(outDir, file));
}

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

const SKIP = /^(?:[a-z]+:|#|\/\/)/i;

export function checkLinks(outDir) {
  const missing = [];
  for (const file of walk(outDir).filter((f) => f.endsWith(".html"))) {
    const html = readFileSync(file, "utf8");
    const baseHref = html.match(/<base href="([^"]+)"/)?.[1];
    const from = baseHref ? outDir : dirname(file);
    for (const [, url] of html.matchAll(/\s(?:src|href|poster)="([^"]+)"/g)) {
      if (SKIP.test(url) || url === "") continue;
      const local = baseHref ? url.replace(/^\/finoinc\//, "") : url;
      const target = resolve(from, local.split(/[?#]/)[0]);
      const ok = existsSync(target) && (!statSync(target).isDirectory() || existsSync(join(target, "index.html")));
      if (!ok) missing.push(`${relative(outDir, file)} -> ${url}`);
    }
  }
  const css = join(outDir, "style.css");
  if (existsSync(css)) {
    for (const [, url] of readFileSync(css, "utf8").matchAll(/url\(["']?([^"')]+)["']?\)/g)) {
      if (!SKIP.test(url) && !existsSync(resolve(outDir, url))) missing.push(`style.css -> ${url}`);
    }
  }
  return missing;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  build();
  const missing = checkLinks(join(ROOT, "_site"));
  if (missing.length) {
    console.error("Broken local links:\n" + missing.join("\n"));
    process.exit(1);
  }
  console.log("site built: site/_site");
}
