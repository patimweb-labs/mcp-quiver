// Bildschirmfotos für den Vergleich zwischen Verwaltung und Client-Fenster.
//
// Zwei Aufgaben:
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
//
//   npm run bilder
//
// Die Seiten **hinter** der Anmeldung (Bibliothek, Gruppen, Benutzer,
// Prüfspur, Geräte) entstehen in `bilder-benutzer.mjs`: gegen einen eigenen
// Server mit temporärer Datenbank und einem erfundenen Katalog. Aus einem
// echten Profil wird **kein** Bild mehr erzeugt — weder hier noch dort.

import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";
import { createServer } from "vite";

const ADMIN = fileURLToPath(new URL("../", import.meta.url));
const ZIEL = fileURLToPath(new URL("../../docs/bilder/", import.meta.url));
const THEMEN = ["dark", "light"];
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

await mkdir(ZIEL, { recursive: true });

const browser = await browserStarten();
try {
  await musterBilder(browser);
} finally {
  await browser.close();
}
