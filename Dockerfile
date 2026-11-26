# mcp-quiver-server: ein Container, ein Port (8740), ein Volume (/daten).
# Baut die Admin-Oberfläche (Vue) und den Server (Rust) und liefert beides aus.
#
# Die Laufzeit besteht aus dem Programm und sonst nichts: **keine Shell, keine
# Paketverwaltung** (distroless), Benutzer 10001, Gruppe 999. Ein Eindringling
# findet im Container kein Werkzeug, mit dem er weiterkommt; ein Angriff über
# eine Shell ist ausgeschlossen, weil es keine gibt.
#
# Die Werkzeug-Programme je Plattform kommen aus dem Release von quiver-tools
# (Aufgabe 37/38) und liegen unter /opt/quiver/tools/<system>/<arch>/. Fehlt das
# Release, wird das Abbild **ohne** Programme gebaut und sagt es beim Bauen.
#
# Alle Basis-Images sind auf einen Digest festgelegt (nicht auf einen Tag, der
# sich ändern kann). Erneuern: `docker buildx imagetools inspect <Bild>` und den
# Digest hier ersetzen.
#
# Basis-Images kommen von Docker Hub. Ohne Docker-Hub-Login geht der öffentliche
# Spiegel (gleiche Inhalte, gleiche Digests):
#   docker build --build-arg BASIS=public.ecr.aws/docker/library -t mcp-quiver-server .
ARG BASIS=docker.io/library

# --- Admin-Oberfläche ---------------------------------------------------------
FROM ${BASIS}/node:22-bookworm-slim@sha256:be979e5523c76fa4d5ca4ebd3099078f220aa47ec39c48ad5b5df9314debd6eb AS admin
ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
WORKDIR /src
# Die Oberfläche braucht die Tokens und Schriften aus dem Rust-Teil (eine Quelle).
COPY tokens tokens
COPY crates/oberflaeche/schriften crates/oberflaeche/schriften
COPY admin admin
WORKDIR /src/admin
RUN npm ci --no-audit --no-fund && npm run build

# --- Server -------------------------------------------------------------------
FROM ${BASIS}/rust:1-slim-bookworm@sha256:bb875b0f8ce726053e259034848c471840e96bfe591fd52556178ae9a05e6126 AS server
WORKDIR /src
# rust-toolchain.toml legt die Version fest; rustup holt sie hier.
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates crates
COPY tokens tokens
# Die Marke der Seiten (das Original liegt in `assets/marke`).
COPY assets assets
RUN cargo build --release --locked -p quiver-server \
 && cp target/release/quiver-server /quiver-server

# --- Programme ----------------------------------------------------------------
# Die Programme, die der Server an die Geräte verteilt (Aufgabe 37): die
# Archive des festgelegten Releases von quiver-tools (Aufgabe 38), je System
# und Architektur eines, entpackt nach /programme/<system>/<arch>/.
#
# Die Fassung steht **hier im Repo**, nicht in einer GitHub-Variable: was ein
# Abbild enthält, soll aus dem Commit hervorgehen. `QUIVER_TOOLS_VERSION` leer
# (`--build-arg QUIVER_TOOLS_VERSION=`) baut weiter, nur ohne Programme – das
# Abbild ist auch ohne sie nützlich (Bibliothek, Zuweisung, Anmeldung), die
# Geräte laden dann nichts.
#
# Geprüft wird in `verteilung/programme.sh`: erst alle Prüfsummen gegen
# `SHA256SUMS`, dann entpacken und gegen `LISTE.json`. Fehlt ein Archiv oder
# stimmt etwas nicht, bricht der Bau ab – lieber kein Abbild als ein halbes.
# Dasselbe Skript läuft lokal in einem Temp-Ordner (siehe Tests).
FROM ${BASIS}/debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e AS programme
COPY verteilung/programme.sh /usr/local/bin/programme
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && chmod 755 /usr/local/bin/programme
# Die Fassung des Releases (ohne „v“). Leer = keine Programme.
ARG QUIVER_TOOLS_VERSION=0.2.1
ARG QUIVER_TOOLS_BASIS="https://github.com/patimweb-labs/quiver-tools/releases/download"
ENV QUIVER_TOOLS_VERSION=$QUIVER_TOOLS_VERSION \
    QUIVER_TOOLS_BASIS=$QUIVER_TOOLS_BASIS \
    ZIEL=/programme
RUN programme

# --- Laufzeit vorbereiten -----------------------------------------------------
# Zertifikate (für Entra und Graph) und Zeitzonen kommen aus Debian; das
# distroless-Image bringt beides nicht mit.
FROM ${BASIS}/debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e AS laufzeit
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /daten /tmp \
 && chown 10001:999 /daten /tmp

# --- Laufzeit -----------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12@sha256:e5d81ddde149641e2a9ba55be4545bc125c67de07508b03ba4c22e6eb0ded5aa
COPY --from=laufzeit /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=laufzeit /usr/share/zoneinfo /usr/share/zoneinfo
COPY --from=laufzeit --chown=10001:999 /daten /daten
COPY --from=laufzeit --chown=10001:999 /tmp /tmp
COPY --from=server /quiver-server /usr/local/bin/quiver-server
COPY --from=admin --chown=10001:999 /src/admin/dist /opt/quiver/admin
# Die Programme je Plattform (Aufgabe 37). Was der Betreiber zusätzlich in
# `<daten>/tools/<system>/` legt, geht vor – dort kommt niemand außer root
# heran, und die Weboberfläche ändert daran nichts.
COPY --from=programme --chown=10001:999 /programme /opt/quiver/tools
ENV QUIVER_ADRESSE=0.0.0.0:8740 \
    QUIVER_PROGRAMME=/opt/quiver/tools \
    QUIVER_DATEN=/daten \
    QUIVER_ADMIN_DIST=/opt/quiver/admin \
    RUST_LOG=info
# Zahlen statt Namen: das distroless-Image hat keine /etc/passwd. UID **und**
# GID bleiben so, wie sie schon in vorhandenen Volumes stehen (10001:999) –
# ein vorhandenes Volume funktioniert ohne `chown` von Hand weiter.
USER 10001:999
VOLUME /daten
EXPOSE 8740
# Der Gesundheitsweg läuft über Loopback auf den eigenen Port und ist auch die
# einzige Ausnahme der Host-Prüfung. `docker exec quiver quiver-server …` geht
# weiterhin: die Binärdatei liegt in /usr/local/bin, das im PATH steht.
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/usr/local/bin/quiver-server", "gesund"]
ENTRYPOINT ["/usr/local/bin/quiver-server"]
