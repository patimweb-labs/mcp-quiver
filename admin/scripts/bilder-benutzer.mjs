// Bildschirmfotos der Seiten der Verwaltung – Bibliothek, Gruppen, Benutzer,
// Prüfspur und die Geräte mit eingerichtetem Entra (Aufgaben 21 und 32).
//
// Dafür läuft der **echte** `quiver-server` auf Loopback mit einem temporären
// Datenordner, gebautem `admin/dist` und einem Entra-Nachbau. Zwei Benutzer
// entstehen über den echten Geräte-Weg (start → bestätigen → Tausch); danach
// setzt das Skript eine kleine Bibliothek, einen Satz, Rollen, eine Sperre und
// einige Prüfspur-Einträge, damit die Bilder etwas zeigen. Keine Firmendaten
// und kein echtes Profil: alles `example.test`, die Personen heißen
// „Alex Beispiel“ und „Ben Beispiel“.
//
//   npm run build && cargo build -p quiver-server && npm run bilder:benutzer
//
// Nur einzelne Seiten (die anderen Bilder bleiben dann unangetastet):
//
//   QUIVER_BILD_SEITEN=bibliothek,gruppen npm run bilder:benutzer

import { createHash, randomBytes } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";

import { entraNachbau, GEHEIMNIS } from "./mock-entra.mjs";
import {
  anfrage,
  entraUmgebung,
  serverStarten,
  sitzungscookie,
} from "./server.mjs";

const ZIEL = fileURLToPath(new URL("../../docs/bilder/", import.meta.url));
const DIST = fileURLToPath(new URL("../dist", import.meta.url));
const ROOT_EMAIL = "root@example.test";
const ROOT_PASSWORT = "ein-sehr-langes-testpasswort";

const ALEX = {
  oid: "aaaaaaaa-9999-8888-7777-666666666666",
  email: "alex.beispiel@example.test",
  name: "Alex Beispiel",
};
const BEN = {
  oid: "bbbbbbbb-9999-8888-7777-666666666666",
  email: "ben.beispiel@example.test",
  name: "Ben Beispiel",
};

const THEMEN = ["dark", "light", "mono"];
const SEITEN = [
  { id: "bibliothek", datei: "bibliothek", titel: "Bibliothek" },
  { id: "gruppen", datei: "gruppen", titel: "Gruppen" },
  { id: "benutzer", datei: "benutzer", titel: "Benutzer" },
  { id: "pruefspur", datei: "pruefspur", titel: "Prüfspur" },
  { id: "geraete", datei: "geraete", titel: "Geräte" },
];
/** Nur diese Seiten schreiben; leer heißt alle (`QUIVER_BILD_SEITEN`). */
const AUSWAHL = (process.env.QUIVER_BILD_SEITEN ?? "")
  .split(",")
  .map((wert) => wert.trim())
  .filter(Boolean);
const GEWAEHLT = AUSWAHL.length
  ? SEITEN.filter((seite) => AUSWAHL.includes(seite.id))
  : SEITEN;

/**
 * Eine kleine, erfundene Bibliothek. Genug, um die Seite zu zeigen: drei
 * Einträge laufen, einer ist abgeschaltet. Nur `example.test`, keine echten
 * Server und keine echten Werkzeuge — gebaut wird aus diesem Katalog nichts.
 */
const BIBLIOTHEK = [
  {
    id: "dateien",
    version: "1.0.0",
    titel: "Dateien",
    beschreibung: "Liest und schreibt Dateien in einem freigegebenen Ordner.",
    herkunft: "eigen",
    laufzeit: { art: "stdio", befehl: "quiver-dateien" },
    felder: [
      {
        name: "PFAD",
        beschreibung: "Ordner auf dem Gerät",
        quelle: "client",
        titel: "Ordner",
      },
    ],
  },
  {
    id: "notizen",
    version: "1.0.0",
    titel: "Notizen",
    beschreibung: "Notizen lesen, schreiben und durchsuchen.",
    herkunft: "eigen",
    laufzeit: { art: "stdio", befehl: "quiver-notizen" },
  },
  {
    id: "kalender",
    version: "1.0.0",
    titel: "Kalender",
    beschreibung: "Termine lesen und anlegen, nach Anmeldung im Browser.",
    herkunft: "fremd",
    laufzeit: { art: "remote", url: "https://kalender.example.test/mcp" },
  },
  {
    id: "wetter",
    version: "1.0.0",
    titel: "Wetterdienst",
    beschreibung: "Wetterdaten über einen fremden MCP-Server.",
    herkunft: "fremd",
    laufzeit: { art: "remote", url: "https://wetter.example.test/mcp" },
    aktiv: false,
  },
];

function b64url(bytes) {
  return Buffer.from(bytes).toString("base64url");
}

function pkce() {
  const verifier = b64url(randomBytes(32));
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  return { verifier, challenge };
}

async function browserStarten() {
  for (const optionen of [
    { channel: "chrome", headless: true },
    {
      executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      headless: true,
    },
    { headless: true },
  ]) {
    try {
      return await chromium.launch(optionen);
    } catch {
      // nächster Versuch
    }
  }
  throw new Error("Kein Chrome gefunden (Kanal `chrome` oder fester Pfad).");
}

function alsCookie(wert, url) {
  const [name, ...rest] = wert.split("=");
  return { name, value: rest.join("="), url };
}

async function kontext(browser, breite, hoehe, thema, cookie, basis) {
  const context = await browser.newContext({
    viewport: { width: breite, height: hoehe },
    locale: "de-DE",
    timezoneId: "Europe/Berlin",
    deviceScaleFactor: 1,
  });
  await context.addInitScript(
    ({ thema }) => localStorage.setItem("quiver-thema", thema),
    { thema },
  );
  await context.addCookies([alsCookie(cookie, basis)]);
  return context;
}

/**
 * Ein Benutzer meldet sich am Gerät an – derselbe Weg wie im Client, nur ohne
 * Fenster: start → Entra-Nachbau → Bestätigungsseite → Code → Tausch.
 */
async function geraetAnmelden(context, basis, geraeteName) {
  const { verifier, challenge } = pkce();
  const zielPort = 41234;
  const start =
    `${basis}/auth/geraet/start?challenge=${challenge}` +
    `&port=${zielPort}&zustand=bilder&name=${encodeURIComponent(geraeteName)}`;
  const seite = await context.request.get(start, { maxRedirects: 20 });
  if (!seite.ok()) {
    throw new Error(`Geräte-Anmeldung: ${seite.status()} ${await seite.text()}`);
  }
  const html = await seite.text();
  const token = /name="token" value="([^"]+)"/.exec(html)?.[1];
  const csrf = /name="csrf" value="([^"]+)"/.exec(html)?.[1];
  if (!token || !csrf) {
    throw new Error("Bestätigungsseite ohne token/csrf");
  }
  const bestaetigt = await context.request.post(
    `${basis}/auth/geraet/bestaetigen`,
    {
      form: { token, csrf },
      headers: { Origin: basis },
      maxRedirects: 0,
    },
  );
  const ziel = bestaetigt.headers()["location"] ?? "";
  const code = new URL(ziel).searchParams.get("code");
  if (!code) throw new Error(`kein Code im Rücksprung: ${ziel}`);
  const tausch = await context.request.post(
    `${basis}/api/v1/auth/geraet/tausch`,
    { data: { code, verifier } },
  );
  if (!tausch.ok()) {
    throw new Error(`Tausch: ${tausch.status()} ${await tausch.text()}`);
  }
  const antwort = await tausch.json();
  if (!antwort.token) throw new Error("Tausch ohne Token");
  return antwort;
}

async function bild(browser, { thema, breite, hoehe, seite, datei, cookie, basis }) {
  const context = await kontext(browser, breite, hoehe, thema, cookie, basis);
  try {
    const page = await context.newPage();
    await page.goto(`${basis}/#/${seite.id}`, { waitUntil: "domcontentloaded" });
    await page
      .getByRole("heading", { level: 1, name: seite.titel })
      .waitFor({ timeout: 15000 });
    if (seite.id === "benutzer") {
      try {
        await page.getByRole("row", { name: /Alex/ }).waitFor();
      } catch (fehler) {
        console.log("DEBUG Benutzer:\n" + (await page.locator("body").innerText()));
        throw fehler;
      }
    }
    if (seite.id === "bibliothek") {
      // Erst wenn die Einträge da sind, zeigt das Bild etwas.
      await page
        .getByRole("heading", { level: 3, name: "Dateien" })
        .waitFor();
    }
    if (seite.id === "pruefspur") {
      await page.getByRole("button", { name: "Nachladen" }).waitFor();
    }
    if (seite.id === "geraete") {
      await page
        .getByRole("heading", {
          level: 2,
          name: "So meldet sich der Benutzer am Gerät an",
        })
        .waitFor();
    }
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(300);
    const pfad = `${ZIEL}${datei}`;
    await page.screenshot({ path: pfad, fullPage: false });
    console.log(pfad);
  } finally {
    await context.close();
  }
}

async function main() {
  await mkdir(ZIEL, { recursive: true });
  const ordner = await mkdtemp(join(tmpdir(), "quiver-bilder-"));
  const geheimnisDatei = join(ordner, "entra-secret");
  await writeFile(geheimnisDatei, GEHEIMNIS, { mode: 0o600 });
  const nachbau = await entraNachbau({ objektId: ALEX.oid });
  const server = await serverStarten({
    dist: DIST,
    umgebung: entraUmgebung(nachbau, geheimnisDatei),
  });
  let browser;
  let vorbereitung;
  try {
    // Root anlegen (Loopback: ohne Code) und eine Entra-Gruppe zuordnen.
    const eingerichtet = await anfrage(server.basis, "/auth/einrichten", {
      methode: "POST",
      kopf: { Origin: server.basis },
      rumpf: { email: ROOT_EMAIL, passwort: ROOT_PASSWORT },
    });
    if (eingerichtet.status !== 200) {
      throw new Error(`Einrichten: ${eingerichtet.status} ${eingerichtet.text}`);
    }
    const cookie = sitzungscookie(eingerichtet);
    if (!cookie) throw new Error("kein Sitzungscookie beim Einrichten");

    // Erst die Bibliothek, dann der Satz (er verweist auf die Einträge), dann
    // die Gruppe (sie bekommt den Satz). Alles erfunden und neutral.
    for (const eintrag of BIBLIOTHEK) {
      const gesetzt = await anfrage(
        server.basis,
        `/api/v1/admin/eintraege/${eintrag.id}`,
        {
          methode: "PUT",
          cookie,
          kopf: { Origin: server.basis },
          rumpf: eintrag,
        },
      );
      if (gesetzt.status !== 200) {
        throw new Error(
          `Eintrag ${eintrag.id}: ${gesetzt.status} ${gesetzt.text}`,
        );
      }
    }
    const satz = await anfrage(server.basis, "/api/v1/admin/saetze/basis", {
      methode: "PUT",
      cookie,
      kopf: { Origin: server.basis },
      rumpf: {
        id: "basis",
        titel: "Basis",
        eintraege: BIBLIOTHEK.map((eintrag) => ({ eintrag: eintrag.id })),
      },
    });
    if (satz.status !== 200) {
      throw new Error(`Satz: ${satz.status} ${satz.text}`);
    }

    const gruppe = await anfrage(server.basis, "/api/v1/admin/gruppen/team", {
      methode: "PUT",
      cookie,
      kopf: { Origin: server.basis },
      rumpf: {
        id: "team",
        titel: "Team",
        saetze: ["basis"],
        entra: [nachbau.gruppe],
      },
    });
    if (gruppe.status !== 200) {
      throw new Error(`Gruppe: ${gruppe.status} ${gruppe.text}`);
    }
    // Eine zweite, rein lokale Gruppe — so zeigt das Bild beide Arten.
    const lokal = await anfrage(
      server.basis,
      "/api/v1/admin/gruppen/entwicklung",
      {
        methode: "PUT",
        cookie,
        kopf: { Origin: server.basis },
        rumpf: {
          id: "entwicklung",
          titel: "Entwicklung",
          saetze: ["basis"],
          entra: [],
        },
      },
    );
    if (lokal.status !== 200) {
      throw new Error(`Gruppe: ${lokal.status} ${lokal.text}`);
    }

    browser = await browserStarten();
    vorbereitung = await kontext(
      browser,
      1440,
      900,
      "dark",
      cookie,
      server.basis,
    );

    // Zwei Benutzer über den echten Geräte-Weg; danach Rollen und eine Sperre.
    nachbau.setzeIdentitaet({ objektId: ALEX.oid, email: ALEX.email, name: ALEX.name });
    await geraetAnmelden(vorbereitung, server.basis, "Arbeitsplatz Alex");
    nachbau.setzeIdentitaet({ objektId: BEN.oid, email: BEN.email, name: BEN.name });
    await geraetAnmelden(vorbereitung, server.basis, "Notebook Ben");

    await anfrage(
      server.basis,
      `/api/v1/admin/entra-gruppen/${nachbau.gruppe}`,
      {
        methode: "PUT",
        cookie,
        kopf: { Origin: server.basis },
        rumpf: { anzeigename: "Alle Mitarbeiter mit verwalteten Geräten" },
      },
    );
    await anfrage(server.basis, `/api/v1/admin/benutzer/${ALEX.oid}`, {
      methode: "PUT",
      cookie,
      kopf: { Origin: server.basis },
      rumpf: { rolle: "admin" },
    });
    await anfrage(server.basis, `/api/v1/admin/benutzer/${BEN.oid}`, {
      methode: "PUT",
      cookie,
      kopf: { Origin: server.basis },
      rumpf: { gesperrt: true },
    });

    // Prüfspur füllen: Anmeldungen, Nachprüfungen und ein Fehlschlag. Der
    // Nachbau muss wieder Anna ausstellen – das neue ID-Token muss dieselbe
    // Identität haben wie das gespeicherte Refresh-Token.
    await anfrage(server.basis, "/api/v1/auth/root", {
      methode: "POST",
      kopf: { Origin: server.basis },
      rumpf: { email: ROOT_EMAIL, passwort: ROOT_PASSWORT },
    });
    nachbau.setzeIdentitaet({ objektId: ALEX.oid, email: ALEX.email, name: ALEX.name });
    for (let i = 0; i < 11; i += 1) {
      const geprueft = await anfrage(
        server.basis,
        `/api/v1/admin/benutzer/${ALEX.oid}/pruefen`,
        { methode: "POST", cookie, kopf: { Origin: server.basis } },
      );
      if (geprueft.status !== 200) {
        throw new Error(`Prüfung ${i}: ${geprueft.status} ${geprueft.text}`);
      }
    }
    await anfrage(server.basis, "/api/v1/auth/root", {
      methode: "POST",
      kopf: { Origin: server.basis },
      rumpf: { email: ROOT_EMAIL, passwort: "falsches-passwort-123" },
    });
    await vorbereitung.close();
    vorbereitung = undefined;

    for (const thema of THEMEN) {
      for (const seite of GEWAEHLT) {
        await bild(browser, {
          thema,
          breite: 1440,
          hoehe: 900,
          seite,
          datei: `admin-${thema}-${seite.datei}.png`,
          cookie,
          basis: server.basis,
        });
      }
    }

    // Die Seiten einmal schmal (560 px), wie in Aufgabe 18.
    for (const seite of GEWAEHLT) {
      await bild(browser, {
        thema: "dark",
        breite: 560,
        hoehe: 900,
        seite,
        datei: `admin-dark-${seite.datei}-560.png`,
        cookie,
        basis: server.basis,
      });
    }
  } finally {
    if (vorbereitung) await vorbereitung.close().catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await nachbau.stoppen().catch(() => {});
    await server.stoppen().catch(() => {});
    await rm(ordner, { recursive: true, force: true });
  }
}

await main();
