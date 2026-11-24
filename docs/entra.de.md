# Microsoft Entra ID einrichten

Zwei Dinge haben in quiver mit Entra zu tun. Sie sind **getrennt**, jedes hat seine eigene App-Registrierung:

| | quiver-Server (diese Anleitung) | Tool Microsoft 365 |
|---|---|---|
| Wofür | Wer darf sich anmelden, welche Tools bekommt er, wer ist Admin | Mails, Kalender, Teams, Dateien lesen und schreiben |
| App | eine eigene im Tenant | mitgeliefert oder eigene |
| Art | Web-App, Server weist sich mit Zertifikat aus | öffentlicher Client, kein Ausweis |
| Rechte | Anmelden, Profil, E-Mail, optional Gruppennamen | Microsoft Graph im Namen des Benutzers |
| Anleitung | hier | [quiver-tools: docs/m365/entra-app.md](https://github.com/patimweb-labs/quiver-tools/blob/main/docs/m365/entra-app.md) |

Ohne Entra läuft quiver auch: Dann gibt es nur den Root-Benutzer und Einmal-Codes für Geräte.

## Was du brauchst

- Einen quiver-Server mit eingerichtetem Root ([README](../README.de.md)).
- Eine Adresse: `https://quiver.firma.de`. `http` geht nur auf dem eigenen Rechner (`http://localhost:8740`).
- Im Tenant eine Rolle, die Apps anlegen und Zustimmung erteilen darf (z. B. **Anwendungsadministrator** oder **Cloudanwendungsadministrator**).
- Um **Gruppen** einer App zuzuweisen, braucht der Tenant **Entra ID P1** oder höher.

Im Folgenden: `<ADRESSE>` = Adresse des Servers, `<TENANT>` = Mandanten-ID, `<APP>` = Anwendungs-ID.

## 1. App registrieren

Im [Entra Admin Center](https://entra.microsoft.com): **Identität → Anwendungen → App-Registrierungen → Neue Registrierung**

| Feld | Wert |
|---|---|
| Name | `quiver` |
| Unterstützte Kontotypen | **Nur Konten in diesem Organisationsverzeichnis** (ein Tenant) |
| Umleitungs-URI | Plattform **Web**, `<ADRESSE>/auth/callback` |

Auf der Übersicht **Anwendungs-ID (Client)** und **Verzeichnis-ID (Mandant)** notieren.

Unter **Authentifizierung** prüfen: „Zugriffstoken“ und „ID-Token“ bei der impliziten Genehmigung bleiben **aus**, „Öffentliche Clientflows zulassen“ bleibt **Nein**. quiver nutzt den Autorisierungscode mit PKCE und braucht beides nicht.

## 2. Gruppen ins Token

**Tokenkonfiguration → Gruppenanspruch hinzufügen**

- **Der Anwendung zugewiesene Gruppen** wählen. Nur diese stehen dann im Token, nicht alle Gruppen eines Benutzers.
- Unter **ID**: **Gruppen-ID**. Keine Namen: Rechte laufen über die ID, weil sich Namen ändern lassen.

Im Manifest steht danach `"groupMembershipClaims": "ApplicationGroup"`.

## 3. Rechte und Zustimmung

**API-Berechtigungen → Berechtigung hinzufügen → Microsoft Graph → Delegierte Berechtigungen**

| Recht | Nötig | Wofür |
|---|---|---|
| `openid`, `profile`, `email` | ja | Anmeldung, Name, E-Mail |
| `offline_access` | ja | stündlich nachprüfen, ob der Benutzer noch darf |
| `GroupMember.Read.All` | nur für Gruppennamen | Anzeigename der Gruppen aus dem Token |

Danach **Administratorzustimmung für … erteilen**. `User.Read` ist nicht nötig.

> Ohne `GroupMember.Read.All` bleibt `QUIVER_ENTRA_GRUPPENNAMEN` aus, und Gruppen heißen zunächst „Entra-Gruppe <erste 8 Zeichen>“. Den Schalter **erst nach** der Zustimmung einschalten, sonst scheitert jede Anmeldung.

## 4. Gruppen und Benutzer zuweisen

**Unternehmensanwendungen → quiver → Benutzer und Gruppen → Hinzufügen**

- Jede Gruppe zuweisen, über die Tools verteilt werden.
- Es zählen **nur direkte Mitglieder**. Verschachtelte Gruppen kommen nicht ins Token.

**Eigenschaften → Zuweisung erforderlich?**

- **Nein**: Jeder im Tenant kann sich anmelden, bekommt ohne Gruppe aber keine Tools.
- **Ja**: Nur Zugewiesene können sich anmelden; andere sehen `AADSTS50105`.

## 5. Ausweis: Zertifikat (empfohlen)

Viele Tenants verbieten Client-Geheimnisse per Richtlinie. Der Server erzeugt seinen Schlüssel selbst, der private Teil verlässt ihn nie:

```bash
docker exec quiver quiver-server entra-schluessel
```

Die Ausgabe enthält **nur** das öffentliche Zertifikat. Den Block von `-----BEGIN CERTIFICATE-----` bis `-----END CERTIFICATE-----` als `quiver.crt` speichern und unter **Zertifikate & Geheimnisse → Zertifikate → Hochladen** einfügen. Prüfen: Der Ablauf muss zur Ausgabe passen.

Alternative **Geheimnis** (wenn erlaubt): unter **Geheime Clientschlüssel** anlegen und auf dem Server in `<Daten>/entra-secret` ablegen (Rechte 0600). Nie als Umgebungsvariable. **Beides zugleich geht nicht**, dann startet der Server nicht.

## 6. Server einschalten

```bash
docker run -d --name quiver --restart unless-stopped \
  -p 127.0.0.1:8740:8740 \
  -e QUIVER_OEFFENTLICH=<ADRESSE> \
  -e QUIVER_ENTRA_TENANT=<TENANT> \
  -e QUIVER_ENTRA_CLIENT_ID=<APP> \
  -v quiver-daten:/daten mcp-quiver-server
```

Im Log steht „Entra-Mandant …“. Ab jetzt gibt es **keine Geräte-Codes** mehr. Geräte, die vorher mit Code verbunden waren, laufen weiter, bis `QUIVER_ALTGERAETE=gesperrt` gesetzt ist.

Hinter einem Reverse-Proxy: TLS dort beenden, `QUIVER_PROXIES` auf das Netz des Proxys setzen, und `Host` unverändert durchreichen.

## 7. In quiver fertig einrichten

1. **Erste Anmeldung** am Client: Verbindung → Adresse → **Mit Microsoft anmelden** → Gerät bestätigen. Tools kommen noch keine.
2. In der Verwaltung (als Root) unter **Gruppen** erscheint die Entra-Gruppe von selbst. Einen **Satz** zuordnen → die Tools erscheinen am Gerät.
3. Unter **Benutzer** → „…“ → **Zum Admin**. Danach klappt auch „Mit Microsoft anmelden“ in der Verwaltung.

Root bleibt als **Notzugang**, falls Entra ausfällt.

## Was wann wirkt

| Änderung in Entra | Wirkung in quiver |
|---|---|
| Benutzer in Gruppe / aus Gruppe | spätestens nach rund 70 min, sofort mit „Jetzt prüfen“ |
| Konto deaktiviert, Sitzungen widerrufen | spätestens nach rund 70 min, sofort mit „Jetzt prüfen“; alle Tools aus |
| Neue Gruppe zugewiesen | erscheint mit der nächsten Anmeldung oder Prüfung eines Mitglieds, ohne Tools |
| Gruppe umbenannt | spätestens nach einem Tag; ein eigener Titel bleibt |
| Entra nicht erreichbar | letzter Stand, höchstens 24 h |

## Zertifikat erneuern (jährlich, ohne Ausfall)

Ab 30 Tagen vor Ablauf warnt der Server in Log, Verwaltung und Prüfspur.

1. `docker exec quiver quiver-server entra-schluessel --erneuern` → neues Zertifikat **zusätzlich** in Entra hochladen
2. `docker exec quiver quiver-server entra-schluessel --umschalten` → wirkt ohne Neustart
3. Anmeldung prüfen, dann das alte Zertifikat in Entra löschen

## Dasselbe mit der Azure CLI

```bash
az login --tenant <TENANT>
cat > rechte.json <<'JSON'
[{"resourceAppId":"00000003-0000-0000-c000-000000000000","resourceAccess":[
 {"id":"37f7f235-527c-4136-accd-4a02d197296e","type":"Scope"},
 {"id":"14dad69e-099b-42c9-810b-d002981feec1","type":"Scope"},
 {"id":"64a6cdd6-aab1-4aaf-94b8-3cc8405e90d0","type":"Scope"},
 {"id":"7427e0e9-2fba-42fe-b0c0-848c9e6a8182","type":"Scope"}]}]
JSON
echo '{"idToken":[{"name":"groups","essential":false,"additionalProperties":[]}]}' > anspruch.json

APP=$(az ad app create --display-name quiver --sign-in-audience AzureADMyOrg \
  --web-redirect-uris <ADRESSE>/auth/callback \
  --enable-id-token-issuance false --enable-access-token-issuance false \
  --required-resource-accesses @rechte.json --optional-claims @anspruch.json \
  --query appId -o tsv)
OBJ=$(az ad app show --id "$APP" --query id -o tsv)
az rest --method patch --url "https://graph.microsoft.com/v1.0/applications/$OBJ" \
  --headers Content-Type=application/json --body '{"groupMembershipClaims":"ApplicationGroup"}'
SP=$(az ad sp create --id "$APP" --query id -o tsv)

# nur für Gruppennamen: GroupMember.Read.All
az ad app permission add --id "$APP" --api 00000003-0000-0000-c000-000000000000 \
  --api-permissions bc024368-1153-4739-b217-4326f2e966d0=Scope

sleep 30   # Entra braucht einen Moment, sonst „Bad Request“
az ad app permission admin-consent --id "$APP"

# Gruppe zuweisen
G=$(az ad group show --group "<GRUPPE>" --query id -o tsv)
az rest --method post --url "https://graph.microsoft.com/v1.0/servicePrincipals/$SP/appRoleAssignedTo" \
  --headers Content-Type=application/json \
  --body "{\"principalId\":\"$G\",\"resourceId\":\"$SP\",\"appRoleId\":\"00000000-0000-0000-0000-000000000000\"}"

# öffentliches Zertifikat hochladen
docker exec quiver cat /daten/entra-schluessel.crt.pem > quiver.crt
az ad app credential reset --id "$APP" --append --cert @quiver.crt --display-name quiver-server
```

## Wenn es klemmt

| Meldung | Ursache und Abhilfe |
|---|---|
| `AADSTS50011` | Umleitungs-URI passt nicht genau. Muss `<ADRESSE>/auth/callback` sein, Plattform **Web** |
| `AADSTS50105` | „Zuweisung erforderlich“ ist an, der Benutzer ist nicht zugewiesen (Schritt 4) |
| `AADSTS65001`, `AADSTS90094`, „Administratorgenehmigung erforderlich“ | Zustimmung fehlt (Schritt 3). Nach dem Einschalten der Gruppennamen: `GroupMember.Read.All` |
| `AADSTS700016` | Falsche App-ID oder falscher Tenant in der Server-Konfiguration |
| `AADSTS700027`, `invalid_client` | Das Zertifikat am Server ist nicht (mehr) in Entra hinterlegt, oder es ist abgelaufen |
| `AADSTS7000215` | Geheimnis falsch: Es zählt der **Wert**, nicht die Geheimnis-ID |
| „Credential type not allowed as per assigned policy“ | Der Tenant verbietet Geheimnisse. Zertifikat nehmen (Schritt 5) |
| „groups-Überlauf“ | Zu viele Gruppen im Token. In Schritt 2 „Der Anwendung zugewiesene Gruppen“ wählen |
| Angemeldet, aber keine Tools | Gruppe hat noch keinen Satz, oder der Benutzer ist nur über eine Untergruppe Mitglied |
| Gruppe erscheint nicht | Noch kein direktes Mitglied hat sich angemeldet, oder sie ist der App nicht zugewiesen |
| „Kein Zugriff“ in der Verwaltung | Dem Konto fehlt die Rolle Admin (Schritt 7) |
| „Host passt nicht zur Basis-URL“ | Genau die Adresse aus `QUIVER_OEFFENTLICH` aufrufen (`localhost` ist nicht `127.0.0.1`) |

## Sicherheit

- Der Client spricht nie mit Microsoft, nur der Server. Agenten sehen kein Token.
- Identität ist Tenant und Objekt-ID. E-Mail und Name sind nur Anzeige.
- Refresh-Tokens liegen verschlüsselt am Server; der Schlüssel `<Daten>/schluessel` gehört in die Sicherung, aber getrennt von der Datenbank.
- Wird der Server kompromittiert: Zertifikat in Entra löschen, „Sitzungen widerrufen“ für die Benutzer, neu aufsetzen.
