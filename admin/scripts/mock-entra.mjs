// Ein Entra-Nachbau für die Browser-Prüfung (Aufgabe 26, Punkt 2): Discovery,
// authorize, token und JWKS auf Loopback, mit einem eigenen RSA-Schlüssel.
//
// Kein echter Mandant, keine Firmendaten: Mandant, Client-Id und Objekt-Ids
// sind erfundene GUIDs, das Geheimnis ist eine Zeichenkette im Testordner.

import { createServer } from "node:http";
import { createHash, createSign, generateKeyPairSync, randomUUID } from "node:crypto";

const TENANT = "11111111-2222-3333-4444-555555555555";
const CLIENT = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
/** Die Entra-Gruppe, die der Nachbau in `groups` behauptet. */
export const GRUPPE = "12345678-1234-1234-1234-123456789012";
/** Das Geheimnis des Nachbaus (nur Testdaten, steht in einer Datei mit 0600). */
export const GEHEIMNIS = "kein-echtes-geheimnis-nur-test";

function b64url(wert) {
  return Buffer.from(wert).toString("base64url");
}

/**
 * Den Nachbau starten. `objektId` ist die Identität, die jede Anmeldung
 * behauptet; `onAnmelden` wird je authorize-Aufruf gerufen (zum Protokollieren).
 */
export async function entraNachbau({ objektId = randomUUID(), onAnmelden } = {}) {
  const { privateKey, publicKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  const jwk = publicKey.export({ format: "jwk" });
  const kid = "nachbau-1";
  const codes = new Map();
  let basis = "";

  const signieren = (claims) => {
    const kopf = b64url(JSON.stringify({ alg: "RS256", typ: "JWT", kid }));
    const rumpf = b64url(JSON.stringify(claims));
    const unterschrift = createSign("RSA-SHA256");
    unterschrift.update(`${kopf}.${rumpf}`);
    unterschrift.end();
    return `${kopf}.${rumpf}.${unterschrift.sign(privateKey).toString("base64url")}`;
  };

  const idToken = (nonce, mitGruppen = true) =>
    signieren({
      iss: basis,
      aud: CLIENT,
      tid: TENANT,
      oid: objektId,
      sub: objektId,
      nonce,
      exp: Math.floor(Date.now() / 1000) + 3600,
      nbf: Math.floor(Date.now() / 1000) - 10,
      iat: Math.floor(Date.now() / 1000) - 10,
      preferred_username: "test@example.test",
      name: "Test Person",
      ...(mitGruppen ? { groups: [GRUPPE] } : {}),
    });

  const server = createServer(async (anfrage, antwort) => {
    const adresse = new URL(anfrage.url, basis);
    const form = async () => {
      const teile = [];
      for await (const stueck of anfrage) teile.push(stueck);
      return new URLSearchParams(Buffer.concat(teile).toString("utf8"));
    };
    const json = (status, wert) => {
      antwort.writeHead(status, { "Content-Type": "application/json" });
      antwort.end(JSON.stringify(wert));
    };

    if (adresse.pathname === "/.well-known/openid-configuration") {
      json(200, {
        issuer: basis,
        authorization_endpoint: `${basis}/authorize`,
        token_endpoint: `${basis}/token`,
        jwks_uri: `${basis}/keys`,
      });
      return;
    }
    if (adresse.pathname === "/keys") {
      json(200, {
        keys: [{ kty: "RSA", kid, use: "sig", alg: "RS256", n: jwk.n, e: jwk.e }],
      });
      return;
    }
    if (adresse.pathname === "/authorize") {
      const code = `code-${codes.size + 1}`;
      const nonce = adresse.searchParams.get("nonce") ?? "";
      const state = adresse.searchParams.get("state") ?? "";
      const redirect = adresse.searchParams.get("redirect_uri") ?? "";
      const challenge = adresse.searchParams.get("code_challenge") ?? "";
      codes.set(code, { nonce, challenge, redirect });
      onAnmelden?.({ code, state });
      const ziel = new URL(redirect);
      ziel.searchParams.set("code", code);
      ziel.searchParams.set("state", state);
      antwort.writeHead(302, { Location: ziel.toString() });
      antwort.end();
      return;
    }
    if (adresse.pathname === "/token" && anfrage.method === "POST") {
      const felder = await form();
      const art = felder.get("grant_type");
      const code = codes.get(felder.get("code"));
      if (art === "authorization_code" && code) {
        const erwartet = createHash("sha256")
          .update(felder.get("code_verifier") ?? "")
          .digest("base64url");
        if (erwartet !== code.challenge) {
          json(400, { error: "invalid_grant" });
          return;
        }
        codes.delete(felder.get("code"));
        json(200, {
          id_token: idToken(code.nonce),
          refresh_token: `refresh-${code.nonce.slice(0, 6)}`,
          token_type: "Bearer",
          expires_in: 3600,
        });
        return;
      }
      if (art === "refresh_token") {
        json(200, {
          id_token: idToken(""),
          refresh_token: `refresh-${Math.random().toString(36).slice(2, 8)}`,
          token_type: "Bearer",
          expires_in: 3600,
        });
        return;
      }
      json(400, { error: "invalid_grant" });
      return;
    }
    json(404, { error: "not_found" });
  });

  await new Promise((fertig) => server.listen(0, "127.0.0.1", fertig));
  const adresse = server.address();
  const port = typeof adresse === "object" && adresse ? adresse.port : 0;
  basis = `http://127.0.0.1:${port}`;

  return {
    basis,
    port,
    tenant: TENANT,
    clientId: CLIENT,
    gruppe: GRUPPE,
    geheimnis: GEHEIMNIS,
    objektId,
    stoppen: () =>
      new Promise((fertig) => {
        server.close(() => fertig());
        server.closeAllConnections?.();
      }),
  };
}
