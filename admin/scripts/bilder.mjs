// Bildschirmfotos für den Vergleich zwischen Verwaltung und Client-Fenster.
//
// Drei Aufgaben:
//
// 1. Die Musterseite (`/muster`, nur im Entwicklungsbau, ohne Anmeldung) in
//    1200 × 800 — dasselbe Bild wie `crates/oberflaeche/tests/muster.rs`
//    erzeugt: `docs/bilder/muster-vue-{dark,light}.png`. Dafür startet dieses
//    Skript den Entwicklungs-Server selbst; es braucht weder Server noch
//    Anmeldung.
// 2. Die Seiten vor der Anmeldung (Anmelden, Ersteinrichtung, geschlossen) in
//    1440 × 900: `docs/bilder/admin-{dark,light}-{anmeldung,einrichtung,
//    geschlossen}.png`. Auch dafür genügt der Entwicklungs-Server: die
//    Antworten von `/auth/stand` und `/api/v1/auth/ich` werden im Browser
//    ersetzt (`context.route`), damit jeder Zustand einmal zu sehen ist. Kein
//    Server, keine Zugangsdaten, keine Ports des Betriebs.
// 3. Die Seiten der Verwaltung (Bibliothek, Geräte) in 1440 × 900. Dafür muss
//    `cargo run -p quiver-server` laufen (er liefert `admin/dist` unter `/`),
//    `npm run build` gelaufen sein, es muss einen Root geben und
//    `QUIVER_BILD_EMAIL`/`QUIVER_BILD_PASSWORT` müssen gesetzt sein. Die
//    Anmeldung läuft über die echte Seite; ein Token gibt es nicht mehr
//    (ADR 0002).
//
//   npm run bilder
//   QUIVER_BILD_BASIS=http://127.0.0.1:8740 QUIVER_BILD_EMAIL=… \
//     QUIVER_BILD_PASSWORT=… npm run bilder
//
// Fehlen E-Mail und Passwort, entstehen die Bilder 1 und 2; die Verwaltungs-
// bilder bleiben, wie sie sind.

import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";
import { createServer } from "vite";

const ADMIN = fileURLToPath(new URL("../", import.meta.url));
const ZIEL = fileURLToPath(new URL("../../docs/bilder/", import.meta.url));
const BASIS = process.env.QUIVER_BILD_BASIS ?? "http://127.0.0.1:8740";
const EMAIL = process.env.QUIVER_BILD_EMAIL ?? "";
const PASSWORT = process.env.QUIVER_BILD_PASSWORT ?? "";
const THEMEN = ["dark", "light"];
const SEITEN = [
  { id: "bibliothek", datei: "bibliothek", titel: "Bibliothek" },
  { id: "geraete", datei: "geraete", titel: "Geräte" },
];
/**
 * Die Zustände vor der Anmeldung, jeder mit der Antwort, die `/auth/stand`
 * dafür geben müsste (Aufgabe 25). `titel` ist die sichtbare h1 der Seite.
 */
const ANMELDEFAELLE = [
  {
    datei: "anmeldung",
    titel: "Anmelden",
    stand: {
      eingerichtet: true,
      einrichtung: null,
      code_noetig: false,
      entra: true,
    },
  },
  {
    datei: "einrichtung",
    titel: "Willkommen bei mcp-quiver",
    stand: {
      eingerichtet: false,
      einrichtung: "offen",
      code_noetig: true,
      entra: true,
    },
  },
  {
    datei: "geschlossen",
    titel: "Die Ersteinrichtung ist geschlossen",
    stand: {
      eingerichtet: false,
      einrichtung: "geschlossen",
      code_noetig: false,
      entra: false,
    },
  },
];
const MUSTER_PORT = 5199;

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

async function kontext(browser, breite, hoehe, { thema } = {}) {
  const context = await browser.newContext({
    viewport: { width: breite, height: hoehe },
    deviceScaleFactor: 1,
    locale: "de-DE",
  });
  await context.addInitScript(
    ({ thema }) => {
      if (thema) localStorage.setItem("quiver-thema", thema);
    },
    { thema },
  );
  return context;
}

/** Die Antworten, die es ohne Server nicht gibt: Stand und „niemand“. */
async function standErsetzen(context, stand) {
  await context.route("**/auth/stand", (route) =>
    route.fulfill({ status: 200, json: stand }),
  );
  await context.route("**/api/v1/auth/ich", (route) =>
    route.fulfill({ status: 401, json: { fehler: "Sitzung fehlt" } }),
  );
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
    await anmeldeBilder(browser, adresse);
  } finally {
    await server.close();
  }
}

/** Anmelden, Ersteinrichtung und die geschlossene Seite – ohne Server. */
async function anmeldeBilder(browser, adresse) {
  for (const thema of THEMEN) {
    for (const fall of ANMELDEFAELLE) {
      const context = await kontext(browser, 1440, 900, { thema });
      await standErsetzen(context, fall.stand);
      const seite = await context.newPage();
      await seite.goto(`${adresse}/`, { waitUntil: "domcontentloaded" });
      await seite
        .getByRole("heading", { level: 1, name: fall.titel })
        .waitFor({ timeout: 15000 });
      await seite.waitForTimeout(200);
      const datei = `${ZIEL}admin-${thema}-${fall.datei}.png`;
      await seite.screenshot({ path: datei, fullPage: false });
      console.log(datei);
      await context.close();
    }
  }
}

/** Die Seiten der Verwaltung — sie brauchen den Server mit Daten und Root. */
async function verwaltungBilder(browser) {
  for (const thema of THEMEN) {
    const context = await kontext(browser, 1440, 900, { thema });
    const seite = await context.newPage();
    await seite.goto(`${BASIS}/`, { waitUntil: "domcontentloaded" });
    // Die echte Anmeldung: E-Mail und Passwort des Root, Sitzung ins Cookie.
    await seite.getByLabel("E-Mail").fill(EMAIL);
    await seite.getByLabel("Passwort").fill(PASSWORT);
    await seite.getByRole("button", { name: "Anmelden" }).click();
    await seite.getByRole("heading", { level: 1, name: "Überblick" }).waitFor({
      timeout: 20000,
    });
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
  if (EMAIL && PASSWORT) {
    await verwaltungBilder(browser);
  } else {
    console.warn(
      "Kein Root für die Bilder — die Bilder der Verwaltung bleiben, wie sie " +
        "sind (QUIVER_BILD_EMAIL und QUIVER_BILD_PASSWORT setzen).",
    );
  }
} finally {
  await browser.close();
}
