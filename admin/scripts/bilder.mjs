// Bildschirmfotos der Verwaltung für den Vergleich mit dem egui-Fenster.
//
// Voraussetzung: `cargo run -p quiver-server` läuft (liefert `admin/dist` unter
// `/`), `npm run build` ist gelaufen, und es gibt Daten (Bibliothek/Geräte).
//
//   npm run bilder
//   QUIVER_BILD_BASIS=http://127.0.0.1:8740 npm run bilder
//
// Der Admin-Token kommt aus `QUIVER_ADMIN_TOKEN` oder aus `daten/admin-token`.

import { existsSync } from "node:fs";
import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";

const WURZEL = fileURLToPath(new URL("../../", import.meta.url));
const ZIEL = fileURLToPath(new URL("../../docs/bilder/", import.meta.url));
const BASIS = process.env.QUIVER_BILD_BASIS ?? "http://127.0.0.1:8740";
const THEMEN = ["dark", "light"];
const SEITEN = [
  { id: "bibliothek", datei: "bibliothek", titel: "Bibliothek" },
  { id: "geraete", datei: "geraete", titel: "Geräte" },
];

async function tokenLesen() {
  if (process.env.QUIVER_ADMIN_TOKEN) return process.env.QUIVER_ADMIN_TOKEN.trim();
  const pfad = `${WURZEL}daten/admin-token`;
  if (!existsSync(pfad)) {
    throw new Error(
      `Kein Admin-Token gefunden (${pfad}). Läuft der Server schon einmal?`,
    );
  }
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

const token = await tokenLesen();
await mkdir(ZIEL, { recursive: true });

const browser = await browserStarten();
try {
  for (const thema of THEMEN) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 1,
      locale: "de-DE",
    });
    await context.addInitScript(
      ({ token, thema }) => {
        sessionStorage.setItem("quiver-admin-token", token);
        localStorage.setItem("quiver-thema", thema);
      },
      { token, thema },
    );

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
} finally {
  await browser.close();
}
