// Die Verwaltung gegen den **echten** Server (Aufgabe 26, Punkt 10):
// einrichten, abmelden, anmelden, falsches Passwort, Übersicht laden, einen
// Satz ändern – mit gebautem `admin/dist` und der echten CSP des Servers.
//
// Chrome headless, eigenes temporäres Profil, Loopback, temporärer
// Datenordner. Kein echter Tresor, kein echtes Profil, keine Firmendaten.
//
// Am Ende: 0 CSP-Verstöße und 0 Konsolenfehler, sonst rot.
//
//   npm run build && cargo build -p quiver-server && npm run pruefung

import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";

import { anfrage, serverStarten } from "./server.mjs";

/** Zeitlimit für den ganzen Lauf: Hängen ist ein Fehlschlag. */
const GESAMT_MS = 180000;
/** Zeitlimit für einen Browser-Schritt. */
const SCHRITT_MS = 20000;

const DIST = fileURLToPath(new URL("../dist", import.meta.url));
const EMAIL = "root@example.test";
const PASSWORT = "ein-sehr-langes-testpasswort";
const FALSCH = "dieses-passwort-ist-falsch";

async function browserStarten() {
  const versuche = [
    { channel: "chrome", headless: true },
    {
      executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      headless: true,
    },
    { headless: true },
  ];
  for (const optionen of versuche) {
    try {
      return await chromium.launch(optionen);
    } catch {
      // nächster Versuch
    }
  }
  throw new Error("Kein Chrome gefunden (Kanal `chrome` oder fester Pfad).");
}

const eintrag = {
  id: "wetter",
  version: "1.0.0",
  titel: "Wetter",
  beschreibung: "Prüfeintrag",
  herkunft: "eigen",
  laufzeit: { art: "stdio", befehl: "npx", argumente: [], umgebung: {} },
  felder: [],
  aktiv: true,
};

const satz = {
  id: "basis",
  titel: "Basis",
  prioritaet: 0,
  eintraege: [],
};

async function main() {
  // Ein gebautes `dist` ist Pflicht – sonst prüfen wir nichts.
  try {
    await stat(join(DIST, "index.html"));
  } catch {
    throw new Error(
      `admin/dist fehlt (${DIST}). Zuerst \`npm run build\` im Ordner admin.`,
    );
  }

  const ordner = await mkdtemp(join(tmpdir(), "quiver-pruefung-"));
  const server = await serverStarten({ dist: DIST });
  let browser;
  try {
    browser = await browserStarten();
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      locale: "de-DE",
      timezoneId: "Europe/Berlin",
    });
    context.setDefaultTimeout(SCHRITT_MS);
    const verstoesse = [];
    const konsolen = [];
    await context.addInitScript(() => {
      window.__csp = [];
      document.addEventListener("securitypolicyviolation", (ereignis) => {
        window.__csp.push(
          `${ereignis.violatedDirective} ${ereignis.blockedURI || "-"}`,
        );
      });
    });
    const seite = await context.newPage();
    /** Antworten, die nicht 2xx/3xx sind – außer den absichtlich abgewiesenen. */
    const fehlantworten = [];
    const erwartet = [/\/api\/v1\/auth\/root$/, /\/api\/v1\/auth\/ich$/];
    seite.on("response", (antwort) => {
      if (antwort.status() < 400) return;
      // Das falsche Passwort und „niemand angemeldet“ antworten absichtlich 401.
      if (erwartet.some((muster) => muster.test(new URL(antwort.url()).pathname))) return;
      fehlantworten.push(`${antwort.status()} ${antwort.url()}`);
    });
    seite.on("console", (nachricht) => {
      // Netzwerkfehler meldet Chrome zusätzlich als Konsolenzeile; die echte
      // Prüfung steht in `fehlantworten` (mit Adresse und Status).
      if (nachricht.type() !== "error") return;
      if (/Failed to load resource/.test(nachricht.text())) return;
      konsolen.push(nachricht.text());
    });
    seite.on("pageerror", (fehler) => konsolen.push(String(fehler)));

    // 1. Ersteinrichtung im Browser.
    await seite.goto(`${server.basis}/`, { waitUntil: "domcontentloaded" });
    await seite
      .getByRole("heading", { level: 1, name: "Willkommen bei mcp-quiver" })
      .waitFor();
    await seite.getByLabel("E-Mail").fill(EMAIL);
    await seite.getByLabel("Passwort", { exact: true }).fill(PASSWORT);
    await seite.getByLabel("Passwort wiederholen").fill(PASSWORT);
    await seite.getByRole("button", { name: "Root anlegen und anmelden" }).click();
    await seite.getByRole("heading", { level: 1, name: "Überblick" }).waitFor();
    console.log("ok: eingerichtet und angemeldet");

    // 2. Daten anlegen (mit der Sitzung des Browsers): ein Eintrag, ein Satz.
    const kuchen = await context.cookies();
    const sitzung = kuchen.find((k) => k.name.endsWith("quiver_sitzung"));
    if (!sitzung) throw new Error("kein Sitzungscookie im Browser");
    const cookie = `${sitzung.name}=${sitzung.value}`;
    for (const [pfad, wert] of [
      ["/api/v1/admin/eintraege/wetter", eintrag],
      ["/api/v1/admin/saetze/basis", satz],
    ]) {
      const antwort = await anfrage(server.basis, pfad, {
        methode: "PUT",
        cookie,
        kopf: { Origin: server.basis },
        rumpf: wert,
      });
      if (antwort.status !== 200) {
        throw new Error(`${pfad}: ${antwort.status} ${antwort.text}`);
      }
    }

    // 3. Abmelden.
    await seite.getByRole("button", { name: "Abmelden" }).click();
    await seite.getByRole("heading", { level: 1, name: "Anmelden" }).waitFor();
    console.log("ok: abgemeldet");

    // 4. Falsches Passwort: die Meldung steht auf der Seite, kein Eintritt.
    await seite.getByLabel("E-Mail").fill(EMAIL);
    await seite.getByLabel("Passwort").fill(FALSCH);
    await seite.getByRole("button", { name: "Anmelden" }).click();
    const meldung = seite.getByRole("alert");
    await meldung.waitFor();
    const text = (await meldung.textContent()) ?? "";
    if (!/passwort/i.test(text)) {
      throw new Error(`unerwartete Meldung beim falschen Passwort: ${text}`);
    }
    if (await seite.getByRole("heading", { level: 1, name: "Überblick" }).count()) {
      throw new Error("falsches Passwort hat angemeldet");
    }
    console.log("ok: falsches Passwort abgewiesen");

    // 5. Richtiges Passwort: die Übersicht lädt mit Daten.
    await seite.getByLabel("Passwort").fill(PASSWORT);
    await seite.getByRole("button", { name: "Anmelden" }).click();
    await seite.getByRole("heading", { level: 1, name: "Überblick" }).waitFor();
    await seite.getByText("Einträge", { exact: false }).first().waitFor();
    console.log("ok: angemeldet und Übersicht geladen");

    // 6. Einen Satz ändern: Sätze → Bearbeiten → Titel → Speichern.
    await seite.getByRole("button", { name: "Sätze" }).click();
    await seite.getByRole("heading", { level: 1, name: "Sätze" }).waitFor();
    await seite.getByRole("heading", { level: 3, name: "Basis" }).waitFor();
    await seite.getByRole("button", { name: "Bearbeiten" }).first().click();
    const dialog = seite.getByRole("dialog");
    await dialog.waitFor();
    const titel = dialog.getByLabel("Titel");
    await titel.fill("Basis geändert");
    await dialog.getByRole("button", { name: "Speichern" }).click();
    await seite
      .getByRole("heading", { level: 3, name: "Basis geändert" })
      .waitFor();
    console.log("ok: Satz geändert");

    // 7. Der Server kennt die Änderung (kein Zwischenspeicher im Browser).
    //    Die Sitzung ist nach dem Abmelden eine neue – also neu auslesen.
    const nachLogin = (await context.cookies()).find((k) =>
      k.name.endsWith("quiver_sitzung"),
    );
    if (!nachLogin) throw new Error("kein Sitzungscookie nach der Anmeldung");
    const gespeichert = await anfrage(server.basis, "/api/v1/admin/saetze/basis", {
      cookie: `${nachLogin.name}=${nachLogin.value}`,
    });
    if (gespeichert.json?.titel !== "Basis geändert") {
      throw new Error(`der Satz steht nicht so in der Datenbank: ${gespeichert.text}`);
    }

    // 8. Nichts darf still kaputt sein: 0 Verstöße, 0 Konsolenfehler.
    verstoesse.push(...((await seite.evaluate(() => window.__csp ?? [])) || []));
    await context.close();
    if (verstoesse.length > 0) {
      throw new Error(`CSP-Verstöße: ${verstoesse.join(" | ")}`);
    }
    if (konsolen.length > 0) {
      throw new Error(`Konsolenfehler: ${konsolen.join(" | ")}`);
    }
    if (fehlantworten.length > 0) {
      throw new Error(`unerwartete Fehlerantworten: ${fehlantworten.join(" | ")}`);
    }
    console.log("ok: 0 CSP-Verstöße, 0 Konsolenfehler, 0 Fehlerantworten");
  } finally {
    if (browser) await browser.close().catch(() => {});
    await server.stoppen().catch(() => {});
    await rm(ordner, { recursive: true, force: true });
  }
}

const uhr = setTimeout(() => {
  console.error(`Zeitlimit: die Prüfung hängt seit ${GESAMT_MS} ms.`);
  process.exit(1);
}, GESAMT_MS);
try {
  await main();
} finally {
  clearTimeout(uhr);
}
