Deutsch: [entra.de.md](entra.de.md)

# Setting up Microsoft Entra ID

Two things in quiver involve Entra. They are **separate**, and each has its own app registration:

| | quiver server (this guide) | Microsoft 365 tool |
|---|---|---|
| Purpose | Who may sign in, which tools they get, who is admin | Read and write mail, calendars, Teams, files |
| App | its own in the tenant | bundled or your own |
| Type | Web app, server authenticates with a certificate | public client, no credential |
| Permissions | sign-in, profile, e-mail, optionally group names | Microsoft Graph on behalf of the user |
| Guide | here | [quiver-tools: docs/m365/entra-app.md](https://github.com/patimweb-labs/quiver-tools/blob/main/docs/m365/entra-app.md) |

quiver also runs without Entra: then there is only the root user and one-time codes for devices.

## What you need

- A quiver server with a configured root user ([README](../README.md)).
- An address: `https://quiver.example.test`. `http` only works on the machine itself (`http://localhost:8740`).
- In the tenant, a role that may create apps and grant consent (for example **Application Administrator** or **Cloud Application Administrator**).
- To assign **groups** to an app, the tenant needs **Entra ID P1** or higher.

In the following: `<ADRESSE>` = address of the server, `<TENANT>` = tenant ID, `<APP>` = application ID.

## 1. Register the app

In the [Entra admin center](https://entra.microsoft.com): **Identity → Applications → App registrations → New registration**

| Field | Value |
|---|---|
| Name | `quiver` |
| Supported account types | **Accounts in this organizational directory only** (one tenant) |
| Redirect URI | platform **Web**, `<ADRESSE>/auth/callback` |

On the overview, note down the **Application (client) ID** and the **Directory (tenant) ID**.

Under **Authentication**, check: "Access tokens" and "ID tokens" under implicit grant stay **off**, "Allow public client flows" stays **No**. quiver uses the authorization code flow with PKCE and needs neither.

## 2. Groups into the token

**Token configuration → Add groups claim**

- Choose **Groups assigned to the application**. Only those then appear in the token, not all groups of a user.
- Under **ID**: **Group ID**. No names: permissions go by ID, because names can be changed.

The manifest then contains `"groupMembershipClaims": "ApplicationGroup"`.

## 3. Permissions and consent

**API permissions → Add a permission → Microsoft Graph → Delegated permissions**

| Permission | Required | For what |
|---|---|---|
| `openid`, `profile`, `email` | yes | sign-in, name, e-mail |
| `offline_access` | yes | check hourly whether the user is still allowed |
| `GroupMember.Read.All` | only for group names | display name of the groups from the token |

Then **Grant admin consent for …**. `User.Read` is not needed.

> Without `GroupMember.Read.All`, leave `QUIVER_ENTRA_GRUPPENNAMEN` off; groups are initially called "Entra-Gruppe <first 8 characters>". Switch this on **only after** consent has been granted, otherwise every sign-in fails.

## 4. Assign groups and users

**Enterprise applications → quiver → Users and groups → Add user/group**

- Assign every group through which tools are distributed.
- Only **direct members** count. Nested groups do not appear in the token.

**Properties → Assignment required?**

- **No**: anyone in the tenant can sign in, but gets no tools without a group.
- **Yes**: only assigned users can sign in; others see `AADSTS50105`.

## 5. Credential: certificate (recommended)

Many tenants prohibit client secrets by policy. The server generates its key itself, and the private part never leaves it:

```bash
docker exec quiver quiver-server entra-schluessel
```

The output contains **only** the public certificate. Save the block from `-----BEGIN CERTIFICATE-----` to `-----END CERTIFICATE-----` as `quiver.crt` and upload it under **Certificates & secrets → Certificates → Upload certificate**. Check: the expiry date must match the output.

Alternative **secret** (if allowed): create it under **Client secrets** and store it on the server in `<Daten>/entra-secret` (permissions 0600). Never as an environment variable. **Both at the same time does not work**, the server will not start then.

## 6. Switch on the server

```bash
docker run -d --name quiver --restart unless-stopped \
  -p 127.0.0.1:8740:8740 \
  -e QUIVER_OEFFENTLICH=<ADRESSE> \
  -e QUIVER_ENTRA_TENANT=<TENANT> \
  -e QUIVER_ENTRA_CLIENT_ID=<APP> \
  -v quiver-daten:/daten mcp-quiver-server
```

The log shows "Entra-Mandant …". From now on there are **no device codes** any more. Devices that were connected with a code before keep running until `QUIVER_ALTGERAETE=gesperrt` is set.

Behind a reverse proxy: terminate TLS there, set `QUIVER_PROXIES` to the proxy's network, and pass `Host` through unchanged.

## 7. Finish setup in quiver

1. **First sign-in** in the client: connection → address → **Sign in with Microsoft** → confirm the device. No tools arrive yet.
2. In the administration (as root), the Entra group appears under **Groups** by itself. Assign a **set** → the tools appear on the device.
3. Under **Users** → "…" → **Make admin**. After that, "Sign in with Microsoft" also works in the administration.

Root remains as an **emergency access** in case Entra fails.

## What takes effect when

| Change in Entra | Effect in quiver |
|---|---|
| User added to / removed from a group | after about 70 min at the latest, immediately with "Check now" |
| Account disabled, sessions revoked | after about 70 min at the latest, immediately with "Check now"; all tools off |
| New group assigned | appears with the next sign-in or check of a member, without tools |
| Group renamed | after one day at the latest; a custom title is kept |
| Entra unreachable | last known state, at most 24 h |

## Renew the certificate (annually, without downtime)

From 30 days before expiry, the server warns in the log, in the administration and in the audit trail.

1. `docker exec quiver quiver-server entra-schluessel --erneuern` → upload the new certificate **in addition** in Entra
2. `docker exec quiver quiver-server entra-schluessel --umschalten` → takes effect without a restart
3. Verify sign-in, then delete the old certificate in Entra

## The same with the Azure CLI

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

# only for group names: GroupMember.Read.All
az ad app permission add --id "$APP" --api 00000003-0000-0000-c000-000000000000 \
  --api-permissions bc024368-1153-4739-b217-4326f2e966d0=Scope

sleep 30   # Entra needs a moment, otherwise "Bad Request"
az ad app permission admin-consent --id "$APP"

# assign group
G=$(az ad group show --group "<GRUPPE>" --query id -o tsv)
az rest --method post --url "https://graph.microsoft.com/v1.0/servicePrincipals/$SP/appRoleAssignedTo" \
  --headers Content-Type=application/json \
  --body "{\"principalId\":\"$G\",\"resourceId\":\"$SP\",\"appRoleId\":\"00000000-0000-0000-0000-000000000000\"}"

# upload public certificate
docker exec quiver cat /daten/entra-schluessel.crt.pem > quiver.crt
az ad app credential reset --id "$APP" --append --cert @quiver.crt --display-name quiver-server
```

## When it gets stuck

| Message | Cause and remedy |
|---|---|
| `AADSTS50011` | Redirect URI does not match exactly. It must be `<ADRESSE>/auth/callback`, platform **Web** |
| `AADSTS50105` | "Assignment required" is on, and the user is not assigned (step 4) |
| `AADSTS65001`, `AADSTS90094`, "Admin approval required" | Consent is missing (step 3). After switching on group names: `GroupMember.Read.All` |
| `AADSTS700016` | Wrong app ID or wrong tenant in the server configuration |
| `AADSTS700027`, `invalid_client` | The certificate on the server is not (or no longer) stored in Entra, or it has expired |
| `AADSTS7000215` | Secret is wrong: the **value** counts, not the secret ID |
| "Credential type not allowed as per assigned policy" | The tenant prohibits secrets. Use a certificate (step 5) |
| "groups-Überlauf" | Too many groups in the token. In step 2, choose "Groups assigned to the application" |
| Signed in, but no tools | The group has no set yet, or the user is only a member through a nested group |
| Group does not appear | No direct member has signed in yet, or it is not assigned to the app |
| "Kein Zugriff" in the administration | The account lacks the admin role (step 7) |
| "Host passt nicht zur Basis-URL" | Call exactly the address from `QUIVER_OEFFENTLICH` (`localhost` is not `127.0.0.1`) |

## Security

- The client never talks to Microsoft, only the server does. Agents never see a token.
- Identity is tenant and object ID. E-mail and name are display only.
- Refresh tokens are stored encrypted on the server; the key `<Daten>/schluessel` belongs in the backup, but separate from the database.
- If the server is compromised: delete the certificate in Entra, "revoke sessions" for the users, set up again.
