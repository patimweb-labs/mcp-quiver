// Hilfen für die beiden Browser-Prüfungen (Aufgabe 26, Punkte 2 und 10):
// einen freien Port finden, den echten `quiver-server` starten und wieder
// beenden.
//
// Alles läuft auf Loopback und mit einem temporären Datenordner; Ports
// 8740–8742 bleiben unberührt (die gehören dem Betrieb), deshalb wird ein
// freier Port gesucht. Kein echter Tresor, kein echtes Profil, kein echtes
// Entra.

import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

export const WURZEL = fileURLToPath(new URL("../../", import.meta.url));

/** Ein Port, der gerade frei ist (und nicht zu 8740–8742 gehört). */
export async function freierPort() {
  for (;;) {
    const port = await new Promise((fertig, fehler) => {
      const probe = createServer();
      probe.unref();
      probe.on("error", fehler);
      probe.listen(0, "127.0.0.1", () => {
        const adresse = probe.address();
        const port = typeof adresse === "object" && adresse ? adresse.port : 0;
        probe.close(() => fertig(port));
      });
    });
    if (port >= 8740 && port <= 8742) continue;
    return port;
  }
}

/** Wartet, bis `bedingung()` wahr ist – mit Zeitlimit (Hängen ist Fehlschlag). */
export async function wartenAuf(bedingung, { was, ms = 20000, takt = 100 }) {
  const ende = Date.now() + ms;
  for (;;) {
    try {
      const wert = await bedingung();
      if (wert) return wert;
    } catch {
      // weiter warten
    }
    if (Date.now() > ende) {
      throw new Error(`Zeitlimit: ${was} ist nach ${ms} ms nicht eingetreten`);
    }
    await new Promise((fertig) => setTimeout(fertig, takt));
  }
}

/** Der Pfad des gebauten Servers. */
export function serverBin() {
  if (process.env.QUIVER_BIN) return process.env.QUIVER_BIN;
  const gesetzt = process.env.CARGO_TARGET_DIR;
  const ziel = gesetzt
    ? isAbsolute(gesetzt)
      ? gesetzt
      : join(WURZEL, gesetzt)
    : join(WURZEL, "target");
  return join(ziel, "debug", "quiver-server");
}

/**
 * Den echten Server starten und auf `/auth/stand` warten.
 *
 * `umgebung` wird zusätzlich zur Umgebung gesetzt; `QUIVER_OEFFENTLICH` und
 * `QUIVER_DATEN` setzt diese Hilfe selbst.
 */
export async function serverStarten({ umgebung = {}, dist } = {}) {
  const port = await freierPort();
  const basis = `http://127.0.0.1:${port}`;
  const daten = await mkdtemp(join(tmpdir(), "quiver-pruefung-"));
  const ausgabe = [];
  const kind = spawn(serverBin(), [], {
    env: {
      ...process.env,
      RUST_LOG: "info",
      QUIVER_ADRESSE: `127.0.0.1:${port}`,
      QUIVER_OEFFENTLICH: basis,
      QUIVER_DATEN: daten,
      ...(dist ? { QUIVER_ADMIN_DIST: dist } : {}),
      ...umgebung,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  kind.stdout.setEncoding("utf8");
  kind.stderr.setEncoding("utf8");
  kind.stdout.on("data", (text) => ausgabe.push(text));
  kind.stderr.on("data", (text) => ausgabe.push(text));
  let beendet = false;
  kind.on("exit", () => {
    beendet = true;
  });

  const stoppen = async () => {
    if (!beendet) {
      kind.kill("SIGTERM");
      await wartenAuf(() => beendet, { was: "Server beendet", ms: 10000 });
    }
    await rm(daten, { recursive: true, force: true });
  };

  try {
    await wartenAuf(
      async () => {
        if (beendet) {
          throw new Error(`Server ist beendet:\n${ausgabe.join("")}`);
        }
        const antwort = await fetch(`${basis}/auth/stand`, {
          headers: { Accept: "application/json" },
        });
        return antwort.ok;
      },
      { was: "Server antwortet auf /auth/stand", ms: 30000 },
    );
  } catch (fehler) {
    await stoppen();
    throw fehler;
  }

  return { basis, port, daten, ausgabe, stoppen, log: () => ausgabe.join("") };
}

/** JSON-Aufruf gegen den Server; `cookie` wird mitgeschickt und zurückgegeben. */
export async function anfrage(basis, pfad, { methode = "GET", rumpf, cookie, kopf = {} } = {}) {
  const kopfzeilen = { Accept: "application/json", ...kopf };
  if (rumpf !== undefined) kopfzeilen["Content-Type"] = "application/json";
  if (methode !== "GET") kopfzeilen["X-Quiver"] = "1";
  if (cookie) kopfzeilen.Cookie = cookie;
  const antwort = await fetch(`${basis}${pfad}`, {
    method: methode,
    headers: kopfzeilen,
    body: rumpf === undefined ? undefined : JSON.stringify(rumpf),
    redirect: "manual",
  });
  const text = await antwort.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    // kein JSON
  }
  return { status: antwort.status, json, text, headers: antwort.headers };
}

/** Die Sitzung aus einer Antwort (nur der Name=Wert-Teil). */
export function sitzungscookie(antwort) {
  const alle = antwort.headers.getSetCookie?.() ?? [];
  for (const wert of alle) {
    const erstes = wert.split(";")[0];
    if (erstes.startsWith("quiver_sitzung=")) return erstes;
  }
  return "";
}

/** Eine Umgebung für den Entra-Nachbau (alle drei Werte oder keiner). */
export function entraUmgebung(nachbau, geheimnisDatei) {
  return {
    QUIVER_ENTRA_TENANT: nachbau.tenant,
    QUIVER_ENTRA_CLIENT_ID: nachbau.clientId,
    QUIVER_ENTRA_SECRET_DATEI: geheimnisDatei,
    QUIVER_ENTRA_AUTHORITY: nachbau.basis,
  };
}
