// Die Verwaltung gegen den **echten** Server (Aufgabe 26, Punkt 10):
// einrichten, abmelden, anmelden, falsches Passwort, Übersicht laden, einen
// Satz ändern – mit gebautem `admin/dist` und der echten CSP des Servers.
//
// Aufgabe 21 prüft zusätzlich die neuen Seiten: Benutzer (Rolle, Sperre,
// jetzt prüfen, Suche, Filter, Root nicht änderbar), Prüfspur (Filter,
// Nachladen, nur lesend) und die Geräte ohne Einmal-Codes, sobald Entra
// eingerichtet ist. Dafür läuft ein Entra-Nachbau auf Loopback; der Benutzer
// entsteht über den echten Geräte-Weg (start → bestätigen → Tausch).
//
// Chrome headless, eigenes temporäres Profil, Loopback, temporärer
// Datenordner. Kein echter Tresor, kein echtes Profil, keine Firmendaten.
//
// Am Ende: 0 CSP-Verstöße und 0 Konsolenfehler, sonst rot.
//
//   npm run build && cargo build -p quiver-server && npm run pruefung

import { createHash, randomBytes } from "node:crypto";
import { mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";

import { entraNachbau, GEHEIMNIS, GRUPPE_NAME } from "./mock-entra.mjs";
import {
  anfrage,
  entraUmgebung,
  freierPort,
  serverStarten,
  wartenAuf,
} from "./server.mjs";

/** Zeitlimit für den ganzen Lauf: Hängen ist ein Fehlschlag. */
const GESAMT_MS = 180000;
/** Zeitlimit für einen Browser-Schritt. */
const SCHRITT_MS = 20000;

const DIST = fileURLToPath(new URL("../dist", import.meta.url));
const EMAIL = "root@example.test";
const PASSWORT = "ein-sehr-langes-testpasswort";
const FALSCH = "dieses-passwort-ist-falsch";
/** Die erfundene Identität des Entra-Nachbaus (keine Firmendaten). */
const BENUTZER_OID = "99999999-8888-7777-6666-555555555555";

function b64url(bytes) {
  return Buffer.from(bytes).toString("base64url");
}

function pkce() {
  const verifier = b64url(randomBytes(32));
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  return { verifier, challenge };
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
  const geheimnisDatei = join(ordner, "entra-secret");
  await writeFile(geheimnisDatei, GEHEIMNIS, { mode: 0o600 });
  const nachbau = await entraNachbau({ objektId: BENUTZER_OID });
  const server = await serverStarten({
    dist: DIST,
    umgebung: entraUmgebung(nachbau, geheimnisDatei, { gruppenNamen: true }),
  });
  const ziel = await lauscher();
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

    // 8. Eine Entra-Gruppe anlegen: Der Benutzer bekommt darüber seine Tools.
    const sitzungswert = `${nachLogin.name}=${nachLogin.value}`;
    const gruppe = await anfrage(server.basis, "/api/v1/admin/gruppen/team", {
      methode: "PUT",
      cookie: sitzungswert,
      kopf: { Origin: server.basis },
      rumpf: { id: "team", titel: "Team", saetze: [], entra: [nachbau.gruppe] },
    });
    if (gruppe.status !== 200) {
      throw new Error(`Gruppe: ${gruppe.status} ${gruppe.text}`);
    }

    // 9. Ein Benutzer meldet sich am Gerät an – der echte Weg (start →
    //    bestätigen → Rücksprung → Tausch). Danach gibt es ihn und ein Gerät.
    const { verifier, challenge } = pkce();
    const start = `${server.basis}/auth/geraet/start?challenge=${challenge}` +
      `&port=${ziel.port}&zustand=pruefung-benutzer&name=Pr%C3%BCfrechner`;
    await seite.goto(start, { waitUntil: "domcontentloaded" });
    await seite
      .getByRole("heading", { level: 1, name: "Gerät anmelden" })
      .waitFor();
    await seite.getByRole("button", { name: "Gerät anmelden" }).click();
    await wartenAuf(() => ziel.gesehen.length > 0, {
      was: "Rücksprung auf den Lauscher",
      ms: SCHRITT_MS,
    });
    const ruecksprung = new URL(
      ziel.gesehen[0],
      `http://127.0.0.1:${ziel.port}`,
    );
    const geraetCode = ruecksprung.searchParams.get("code");
    if (!geraetCode) throw new Error(`kein Code im Rücksprung: ${ziel.gesehen[0]}`);
    const tausch = await anfrage(server.basis, "/api/v1/auth/geraet/tausch", {
      methode: "POST",
      rumpf: { code: geraetCode, verifier },
    });
    if (tausch.status !== 200 || !tausch.json?.token) {
      throw new Error(`Geräte-Tausch: ${tausch.status} ${tausch.text}`);
    }
    console.log(`ok: Benutzer und Gerät angelegt (${tausch.json.geraet.id})`);

    // 10. Der Entra-Gruppe einen Anzeigenamen geben (Schlüssel bleibt die Id).
    const gruppenname = await anfrage(
      server.basis,
      `/api/v1/admin/entra-gruppen/${nachbau.gruppe}`,
      {
        methode: "PUT",
        cookie: sitzungswert,
        kopf: { Origin: server.basis },
        rumpf: { anzeigename: "Team" },
      },
    );
    if (gruppenname.status !== 200) {
      throw new Error(`Anzeigename: ${gruppenname.status} ${gruppenname.text}`);
    }

    // 11. Seite Gruppen: der Admin-Titel steht, klein darunter der Name aus
    //     Entra (Aufgabe 30); die Objekt-Id nur im Hinweis, nie als Text.
    //     Der Name kommt aus dem Hintergrund; erst warten, dann laden.
    await wartenAuf(
      async () => {
        const stand = await anfrage(server.basis, "/api/v1/admin/entra-gruppen", {
          cookie: sitzungswert,
        });
        return stand.json?.some(
          (eintrag) =>
            eintrag.id === nachbau.gruppe && eintrag.name_entra === GRUPPE_NAME,
        );
      },
      { was: "Name aus Entra am Server", ms: SCHRITT_MS },
    );
    await seite.goto(`${server.basis}/#/gruppen`, {
      waitUntil: "domcontentloaded",
    });
    await seite.getByRole("heading", { level: 1, name: "Gruppen" }).waitFor();
    await seite.getByRole("heading", { level: 3, name: "Team" }).waitFor();
    await seite
      .getByText(`Entra: ${GRUPPE_NAME}`, { exact: true })
      .waitFor();
    if ((await seite.getByText(nachbau.gruppe, { exact: true }).count()) > 0) {
      throw new Error("die Objekt-Id steht als Text auf der Seite");
    }
    console.log("ok: Gruppe zeigt Titel und Namen aus Entra");

    // 12. Seite Benutzer: Zeile, Suche, Filter, Root nicht änderbar.
    await seite.goto(`${server.basis}/#/benutzer`, {
      waitUntil: "domcontentloaded",
    });
    await seite.getByRole("heading", { level: 1, name: "Benutzer" }).waitFor();
    const zeile = seite.getByRole("row", { name: /Test Person/ });
    await zeile.waitFor();
    const zeilentext = (await zeile.textContent()) ?? "";
    for (const teil of [
      "test@example.test",
      "Mitarbeiter",
      "Team",
      "geprüft",
    ]) {
      if (!zeilentext.includes(teil)) {
        throw new Error(`in der Benutzerzeile fehlt „${teil}“: ${zeilentext}`);
      }
    }
    const rootZeile = seite.getByRole("row", { name: /root@example\.test/ });
    await rootZeile.getByText("Notzugang").waitFor();
    if ((await rootZeile.getByRole("button").count()) > 0) {
      throw new Error("der Root hat Aktionen, darf aber nicht änderbar sein");
    }
    console.log("ok: Benutzerzeile mit Gruppe, Gerät und Prüfstand");

    await seite.getByLabel("Benutzer durchsuchen").fill("gibtsnicht");
    await seite.getByText("Keine Treffer", { exact: true }).waitFor();
    await seite.getByRole("button", { name: "Filter leeren" }).click();
    await seite.getByRole("row", { name: /Test Person/ }).waitFor();
    await seite.getByLabel("Benutzer durchsuchen").fill("anna");
    await seite.getByText("Keine Treffer", { exact: true }).waitFor();
    await seite.getByLabel("Benutzer durchsuchen").fill("");
    console.log("ok: Suche und leerer Trefferzustand");

    // 12. Rolle ändern, sperren, entsperren, jetzt prüfen – jede mit Rückfrage.
    const rueckfrage = seite.getByRole("dialog");
    const waehle = async (punkt) => {
      await seite
        .getByRole("row", { name: /Test Person/ })
        .getByRole("button", { name: /^Aktionen für/ })
        .click();
      const eintrag = seite.getByRole("menuitem", { name: punkt, exact: true });
      await eintrag.waitFor();
      // Das Menü nimmt in den ersten 250 ms keinen Klick an.
      await seite.waitForTimeout(300);
      await eintrag.click();
    };
    await waehle("Zum Admin");
    await rueckfrage.waitFor();
    if (!((await rueckfrage.textContent()) ?? "").includes("wird Admin")) {
      throw new Error("die Rückfrage erklärt das Ernennen nicht");
    }
    await rueckfrage.getByRole("button", { name: "Zum Admin" }).click();
    await zeile.getByText("Admin", { exact: true }).first().waitFor();
    await waehle("Zum Mitarbeiter");
    await rueckfrage.waitFor();
    await rueckfrage.getByRole("button", { name: "Zum Mitarbeiter" }).click();
    await zeile.getByText("Mitarbeiter", { exact: true }).first().waitFor();
    console.log("ok: Admin ernannt und wieder entzogen");

    await waehle("Sperren");
    await rueckfrage.waitFor();
    await rueckfrage.getByRole("button", { name: "Sperren" }).click();
    await zeile.getByText("gesperrt", { exact: true }).waitFor();
    await waehle("Entsperren");
    await rueckfrage.waitFor();
    await rueckfrage.getByRole("button", { name: "Entsperren" }).click();
    await wartenAuf(async () => (await zeile.getByText("gesperrt", { exact: true }).count()) === 0, {
      was: "Benutzer entsperrt",
      ms: SCHRITT_MS,
    });
    console.log("ok: gesperrt und entsperrt");

    await waehle("Jetzt prüfen");
    await rueckfrage.waitFor();
    if (!((await rueckfrage.textContent()) ?? "").includes("nachgeprüft")) {
      throw new Error("die Rückfrage erklärt das Nachprüfen nicht");
    }
    await rueckfrage.getByRole("button", { name: "Jetzt prüfen" }).click();
    await seite.getByText("sind nachgeprüft", { exact: false }).waitFor();
    console.log("ok: jetzt geprüft");

    // 13. Genug Einträge für „Nachladen“: 21 weitere Prüfungen, dann 20 + 1.
    for (let i = 0; i < 21; i += 1) {
      const geprueft = await anfrage(
        server.basis,
        `/api/v1/admin/benutzer/${BENUTZER_OID}/pruefen`,
        { methode: "POST", cookie: sitzungswert, kopf: { Origin: server.basis } },
      );
      if (geprueft.status !== 200) {
        throw new Error(`Prüfung ${i}: ${geprueft.status} ${geprueft.text}`);
      }
    }

    // 14. Seite Prüfspur: nur lesend, Filter und Nachladen.
    await seite.goto(`${server.basis}/#/pruefspur`, {
      waitUntil: "domcontentloaded",
    });
    await seite.getByRole("heading", { level: 1, name: "Prüfspur" }).waitFor();
    await seite.getByRole("button", { name: "Nachladen" }).waitFor();
    const zeilen = seite.locator("table.tabelle tbody tr");
    const vorher = await zeilen.count();
    if (vorher === 0 || vorher > 20) {
      throw new Error(`unerwartet viele Zeilen vor dem Nachladen: ${vorher}`);
    }
    if ((await seite.getByRole("button", { name: "Löschen" }).count()) > 0) {
      throw new Error("die Prüfspur bietet eine Löschaktion");
    }
    await seite.getByRole("button", { name: "Nachladen" }).click();
    await wartenAuf(async () => (await zeilen.count()) > vorher, {
      was: "Nachladen bringt weitere Einträge",
      ms: SCHRITT_MS,
    });
    console.log(`ok: Prüfspur geladen und nachgeladen (${vorher} → mehr)`);

    await seite.getByRole("combobox", { name: "Art" }).click();
    await seite.getByRole("option", { name: "Verwaltung" }).click();
    await wartenAuf(
      async () => {
        const arten = await seite
          .locator("table.tabelle tbody tr td:nth-child(2)")
          .allTextContents();
        return (
          arten.length > 0 && arten.every((art) => art.trim() === "Verwaltung")
        );
      },
      { was: "Filter nach Art Verwaltung", ms: SCHRITT_MS },
    );
    await seite.getByRole("combobox", { name: "Art" }).click();
    await seite.getByRole("option", { name: "Alle Arten" }).click();
    await seite.getByLabel("Prüfspur durchsuchen").fill("gibtsnicht");
    await seite.getByText("Keine Treffer", { exact: true }).waitFor();
    await seite.getByRole("button", { name: "Filter leeren" }).click();
    console.log("ok: Prüfspur filtert nach Art und Suche");

    // 15. Geräte: mit Entra keine Einmal-Codes, dafür die Erklärung; Besitzer.
    await seite.goto(`${server.basis}/#/geraete`, {
      waitUntil: "domcontentloaded",
    });
    await seite.getByRole("heading", { level: 1, name: "Geräte" }).waitFor();
    await seite
      .getByRole("heading", {
        level: 2,
        name: "So meldet sich der Benutzer am Gerät an",
      })
      .waitFor();
    if ((await seite.getByRole("button", { name: "Gerät einbinden" }).count()) > 0) {
      throw new Error("mit Entra gibt es weiter Einmal-Codes");
    }
    await seite.getByRole("columnheader", { name: "Besitzer" }).waitFor();
    await seite.getByText("Prüfrechner", { exact: false }).first().waitFor();
    console.log("ok: Geräte erklären die Anmeldung statt Codes");

    // 16. Löschen – die letzte Handlung, danach ist der Benutzer weg.
    await seite.goto(`${server.basis}/#/benutzer`, {
      waitUntil: "domcontentloaded",
    });
    await seite.getByRole("heading", { level: 1, name: "Benutzer" }).waitFor();
    await seite.getByRole("row", { name: /Test Person/ }).waitFor();
    await waehle("Löschen");
    await rueckfrage.waitFor();
    if (!((await rueckfrage.textContent()) ?? "").includes("wird gelöscht")) {
      throw new Error("die Rückfrage erklärt das Löschen nicht");
    }
    await rueckfrage.getByRole("button", { name: "Löschen" }).click();
    await wartenAuf(
      async () =>
        (await seite.getByRole("row", { name: /Test Person/ }).count()) === 0,
      { was: "Benutzer gelöscht", ms: SCHRITT_MS },
    );
    await seite.getByRole("row", { name: /root@example\.test/ }).waitFor();
    console.log("ok: Benutzer gelöscht, Root bleibt");

    // 17. Nichts darf still kaputt sein: 0 Verstöße, 0 Konsolenfehler.
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
    await ziel.stoppen().catch(() => {});
    await nachbau.stoppen().catch(() => {});
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
