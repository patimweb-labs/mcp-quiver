// Browser-Prüfung der Anmeldung (Aufgabe 26, Punkt 2 und 3): Anmelden →
// Bestätigen → Rücksprung auf genau `http://127.0.0.1:<port>/quiver` mit Code.
//
// Gegen den **echten** Server (gebauter `quiver-server`) und einen
// Entra-Nachbau auf Loopback, mit Chrome headless und eigenem temporären
// Profil. Kein echter Mandant, keine Firmendaten, kein echter Tresor.
//
// Geprüft wird zusätzlich:
// - `Referrer-Policy: same-origin` über die Kopfzeile **und** im Browser (das
//   Formular trägt `Origin` und `Referer` – mit `no-referrer` wäre `Origin`
//   `null` und die Bestätigung würde scheitern),
// - `form-action` genau für den Port des Vorgangs (Chrome prüft die
//   Weiterleitung nach dem Absenden),
// - das Stylesheet der Seiten lädt unter der echten CSP (`style-src 'self'`),
// - der Hand-Cursor auf dem Knopf.
//
//   cargo build -p quiver-server && npm run pruefung:anmeldung

import { createHash, randomBytes } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { chromium } from "playwright";

import { entraNachbau, GEHEIMNIS } from "./mock-entra.mjs";
import {
  anfrage,
  freierPort,
  entraUmgebung,
  sitzungscookie,
  serverStarten,
  wartenAuf,
} from "./server.mjs";

/** Zeitlimit für den ganzen Lauf: Hängen ist ein Fehlschlag. */
const GESAMT_MS = 120000;
/** Zeitlimit für einen Browser-Schritt. */
const SCHRITT_MS = 20000;

const ROOT_EMAIL = "root@example.test";
const ROOT_PASSWORT = "ein-sehr-langes-testpasswort";

function b64url(bytes) {
  return Buffer.from(bytes).toString("base64url");
}

function pkce() {
  const verifier = b64url(randomBytes(32));
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  return { verifier, challenge };
}

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

/** Ein Lauscher auf 127.0.0.1, der den Rücksprung mit Code aufnimmt. */
async function lauscher() {
  const port = await freierPort();
  const gesehen = [];
  const server = createServer((anfrage, antwort) => {
    gesehen.push(anfrage.url ?? "");
    antwort.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
    antwort.end("mcp-quiver: Code angekommen");
  });
  await new Promise((fertig) => server.listen(port, "127.0.0.1", fertig));
  return {
    port,
    gesehen,
    stoppen: () =>
      new Promise((fertig) => {
        server.close(() => fertig());
        server.closeAllConnections?.();
      }),
  };
}

async function main() {
  const ordner = await mkdtemp(join(tmpdir(), "quiver-anmeldung-"));
  const geheimnisDatei = join(ordner, "entra-secret");
  await writeFile(geheimnisDatei, GEHEIMNIS, { mode: 0o600 });

  const nachbau = await entraNachbau();
  const server = await serverStarten({
    umgebung: entraUmgebung(nachbau, geheimnisDatei),
  });
  const ziel = await lauscher();
  let browser;
  try {
    // 1. Kopfzeile: `Referrer-Policy` ist `same-origin`, nicht `no-referrer`.
    const css = await anfrage(server.basis, "/auth/stil.css");
    const referrer = css.headers.get("referrer-policy");
    if (referrer !== "same-origin") {
      throw new Error(`Referrer-Policy ist ${referrer}, erwartet same-origin`);
    }
    const csp = css.headers.get("content-security-policy") ?? "";
    if (!csp.includes("style-src 'self'") || csp.includes("unsafe-inline")) {
      throw new Error(`CSP der Seiten passt nicht: ${csp}`);
    }

    // 2. Einrichten (Loopback: ohne Code) und eine Gruppe mit der
    //    Entra-Id anlegen – sonst bekommt der Benutzer keine Tools.
    const eingerichtet = await anfrage(server.basis, "/auth/einrichten", {
      methode: "POST",
      kopf: { Origin: server.basis },
      rumpf: { email: ROOT_EMAIL, passwort: ROOT_PASSWORT },
    });
    if (eingerichtet.status !== 200) {
      throw new Error(`Einrichten: ${eingerichtet.status} ${eingerichtet.text}`);
    }
    const cookie = sitzungscookie(eingerichtet);
    const gruppe = await anfrage(server.basis, "/api/v1/admin/gruppen/team", {
      methode: "PUT",
      cookie,
      kopf: { Origin: server.basis },
      rumpf: { id: "team", titel: "Team", saetze: [], entra: [nachbau.gruppe] },
    });
    if (gruppe.status !== 200) {
      throw new Error(`Gruppe: ${gruppe.status} ${gruppe.text}`);
    }

    // 3. Der Browser-Weg: Gerät startet, bestätigt, kommt mit Code zurück.
    browser = await browserStarten();
    const context = await browser.newContext({
      viewport: { width: 1000, height: 800 },
      locale: "de-DE",
      timezoneId: "Europe/Berlin",
    });
    context.setDefaultTimeout(SCHRITT_MS);
    const seite = await context.newPage();
    const konsole = [];
    seite.on("console", (nachricht) => {
      if (nachricht.type() === "error") konsole.push(nachricht.text());
    });
    seite.on("pageerror", (fehler) => konsole.push(String(fehler)));

    const { verifier, challenge } = pkce();
    const start = `${server.basis}/auth/geraet/start?challenge=${challenge}` +
      `&port=${ziel.port}&zustand=pruefung-1&name=Pr%C3%BCfrechner`;

    const bestaetigung = seite.waitForResponse(
      (antwort) => antwort.url().startsWith(`${server.basis}/auth/callback`),
      { timeout: SCHRITT_MS },
    );
    await seite.goto(start, { waitUntil: "domcontentloaded" });
    const seiteAntwort = await bestaetigung;
    const cspBestaetigung = seiteAntwort.headers()["content-security-policy"] ?? "";
    if (!cspBestaetigung.includes(`form-action 'self' http://127.0.0.1:${ziel.port}`)) {
      throw new Error(
        `form-action zeigt nicht auf den Port des Vorgangs: ${cspBestaetigung}`,
      );
    }
    if (cspBestaetigung.includes("unsafe-inline")) {
      throw new Error("die Bestätigungsseite erlaubt Inline-Inhalte");
    }

    await seite.getByRole("heading", { level: 1, name: "Gerät anmelden" }).waitFor();
    // Das Stylesheet der Seiten ist geladen (style-src 'self' greift).
    const gestaltet = await seite.evaluate(() => {
      const knopf = document.querySelector("button[type=submit]");
      const stiel = knopf ? getComputedStyle(knopf) : null;
      return {
        bogen: document.styleSheets.length,
        cursor: stiel?.cursor ?? "",
        hintergrund: getComputedStyle(document.body).backgroundColor,
      };
    });
    if (gestaltet.bogen === 0) throw new Error("kein Stylesheet geladen");
    if (gestaltet.cursor !== "pointer") {
      throw new Error(`Hand-Cursor fehlt (${gestaltet.cursor})`);
    }
    if (!gestaltet.hintergrund || gestaltet.hintergrund === "rgba(0, 0, 0, 0)") {
      throw new Error("kein Hintergrund aus den Tokens");
    }

    // Das Formular trägt `Origin` und `Referer` – der Beweis, dass die
    // Referrer-Policy im echten Browser passt.
    const kopfzeilen = [];
    seite.on("request", (anfrage) => {
      if (anfrage.url().endsWith("/auth/geraet/bestaetigen")) {
        kopfzeilen.push(anfrage.headers());
      }
    });
    await seite.getByRole("button", { name: "Gerät anmelden" }).click();

    await wartenAuf(() => ziel.gesehen.length > 0, {
      was: "Rücksprung auf den Lauscher",
      ms: SCHRITT_MS,
    });
    const ruecksprung = ziel.gesehen[0];
    const url = new URL(`http://127.0.0.1:${ziel.port}${ruecksprung}`);
    const code = url.searchParams.get("code");
    if (!code) throw new Error(`kein Code im Rücksprung: ${ruecksprung}`);
    if (url.searchParams.get("zustand") !== "pruefung-1") {
      throw new Error(`zustand fehlt im Rücksprung: ${ruecksprung}`);
    }
    if (!ruecksprung.startsWith("/quiver?")) {
      throw new Error(`Rücksprung geht nicht auf /quiver: ${ruecksprung}`);
    }

    const post = kopfzeilen[0];
    if (!post) throw new Error("der Absende-Aufruf wurde nicht gesehen");
    if (post.origin !== server.basis) {
      throw new Error(`Origin beim Absenden ist ${post.origin}`);
    }
    if (!post.referer?.startsWith(server.basis)) {
      throw new Error(`Referer beim Absenden fehlt: ${post.referer}`);
    }

    // 4. Der Code ist echt: der Tausch liefert ein Geräte-Token.
    const tausch = await anfrage(server.basis, "/api/v1/auth/geraet/tausch", {
      methode: "POST",
      rumpf: { code, verifier },
    });
    if (tausch.status !== 200) {
      throw new Error(`Tausch: ${tausch.status} ${tausch.text}`);
    }
    if (!tausch.json?.token || tausch.json.geraet?.name !== "Prüfrechner") {
      throw new Error(`unerwartete Tausch-Antwort: ${tausch.text}`);
    }

    if (konsole.length > 0) {
      throw new Error(`Konsolenfehler: ${konsole.join(" | ")}`);
    }

    await context.close();
    console.log("ok: Anmelden → Bestätigen → Rücksprung mit Code");
    console.log(`    Referrer-Policy ${referrer}, form-action Port ${ziel.port}`);
    console.log(`    CSP ohne Inline, Stylesheet geladen, Hand-Cursor gesetzt`);
    console.log(`    Tausch gegen Geräte-Token: ${tausch.json.geraet.id}`);
  } finally {
    if (browser) await browser.close().catch(() => {});
    await ziel.stoppen().catch(() => {});
    await server.stoppen().catch(() => {});
    await nachbau.stoppen().catch(() => {});
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
