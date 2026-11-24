// Bildschirmfotos für den Vergleich zwischen Verwaltung und Client-Fenster.
//
// Zwei Aufgaben:
//
// 1. Die Musterseite (`/muster`, nur im Entwicklungsbau, ohne Anmeldung) in
//    1200 × 800 — dasselbe Bild wie `crates/oberflaeche/tests/muster.rs`
//    erzeugt: `docs/bilder/muster-vue-{dark,light}.png`. Dafür startet dieses
//    Skript den Entwicklungs-Server selbst; es braucht weder Server noch
//    Anmeldung.
// 2. Die Seiten der Verwaltung (Bibliothek, Geräte) in 1440 × 900. Dafür muss
//    `cargo run -p quiver-server` laufen (er liefert `admin/dist` unter `/`),
//    `npm run build` gelaufen sein und es muss Daten geben.
//
//   npm run bilder
//   QUIVER_BILD_BASIS=http://127.0.0.1:8740 npm run bilder
//
// Der Admin-Token kommt aus `QUIVER_ADMIN_TOKEN` oder aus `daten/admin-token`.
// Fehlt er, entstehen nur die Musterbilder.

import { existsSync } from "node:fs";
import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";
import { createServer } from "vite";

const ADMIN = fileURLToPath(new URL("../", import.meta.url));
const WURZEL = fileURLToPath(new URL("../../", import.meta.url));
const ZIEL = fileURLToPath(new URL("../../docs/bilder/", import.meta.url));
const BASIS = process.env.QUIVER_BILD_BASIS ?? "http://127.0.0.1:8740";
const THEMEN = ["dark", "light"];
const SEITEN = [
  { id: "bibliothek", datei: "bibliothek", titel: "Bibliothek" },
  { id: "geraete", datei: "geraete", titel: "Geräte" },
];
const MUSTER_PORT = 5199;

async function tokenLesen() {
  if (process.env.QUIVER_ADMIN_TOKEN) return process.env.QUIVER_ADMIN_TOKEN.trim();
  const pfad = `${WURZEL}daten/admin-token`;
  if (!existsSync(pfad)) return null;
  return (await readFile(pfad, "utf8")).trim();
}

async function browserStarten() {
  // Zuerst der installierte Chrome (schnell, kein zusätzlicher Download),
  // sonst das von Playwright verwaltete Chromium.
  for (const optionen of [{ channel: "chrome" }, {}]) {
    try {
      return await chromium.launch(optionen);
    } catch {
      // nächster Versuch
    }
  }
  throw new Error("Kein Browser gefunden. `npx playwright install chromium` hilft.");
}

async function kontext(browser, breite, hoehe, { token, thema } = {}) {
  const context = await browser.newContext({
    viewport: { width: breite, height: hoehe },
    deviceScaleFactor: 1,
    locale: "de-DE",
  });
  await context.addInitScript(
    ({ token, thema }) => {
      if (token) sessionStorage.setItem("quiver-admin-token", token);
      if (thema) localStorage.setItem("quiver-thema", thema);
    },
    { token, thema },
  );
  return context;
}

/** Die Musterseite: derselbe Aufbau wie das Bild des Client-Fensters. */
async function musterBilder(browser) {
  const server = await createServer({
    root: ADMIN,
    logLevel: "error",
    server: { port: MUSTER_PORT, strictPort: true },
  });
  await server.listen();
  // Die Adresse kommt vom Server selbst (er kann auf `localhost` oder
  // `::1` lauschen — je nach System).
  const adresse = server.resolvedUrls.local[0].replace(/\/$/, "");
  try {
    for (const thema of THEMEN) {
      const context = await kontext(browser, 1200, 800, { thema });
      const seite = await context.newPage();
      await seite.goto(`${adresse}/muster`, {
        waitUntil: "domcontentloaded",
      });
      await seite.getByRole("heading", { level: 1, name: "Bibliothek" }).waitFor({
        timeout: 15000,
      });
      await seite.waitForTimeout(300);
      const datei = `${ZIEL}muster-vue-${thema}.png`;
      await seite.screenshot({ path: datei, fullPage: false });
      console.log(datei);
      await context.close();
    }
  } finally {
    await server.close();
  }
}

/** Die Seiten der Verwaltung — sie brauchen den Server mit Daten. */
async function verwaltungBilder(browser, token) {
  for (const thema of THEMEN) {
    const context = await kontext(browser, 1440, 900, { token, thema });
    const seite = await context.newPage();
    for (const ziel of SEITEN) {
      await seite.goto(`${BASIS}/#/${ziel.id}`, { waitUntil: "domcontentloaded" });
      await seite.getByRole("heading", { level: 1, name: ziel.titel }).waitFor({
        timeout: 15000,
      });
      // Auf die Daten warten: entweder die Liste oder ein leerer Zustand.
      await seite.waitForLoadState("networkidle");
      await seite.waitForTimeout(300);
      const datei = `${ZIEL}admin-${thema}-${ziel.datei}.png`;
      await seite.screenshot({ path: datei, fullPage: false });
      console.log(datei);
    }
    await context.close();
  }
}

await mkdir(ZIEL, { recursive: true });

const browser = await browserStarten();
try {
  await musterBilder(browser);
  const token = await tokenLesen();
  if (token) {
    await verwaltungBilder(browser, token);
  } else {
    console.warn(
      "Kein Admin-Token gefunden — die Bilder der Verwaltung bleiben, wie sie sind " +
        `(${WURZEL}daten/admin-token).`,
    );
  }
} finally {
  await browser.close();
}
