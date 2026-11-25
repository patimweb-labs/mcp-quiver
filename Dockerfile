# mcp-quiver-server: ein Container, ein Port (8740), ein Volume (/daten).
# Baut die Admin-Oberfläche (Vue) und den Server (Rust) und liefert beides aus.
#
# Die Laufzeit besteht aus dem Programm und sonst nichts: **keine Shell, keine
# Paketverwaltung** (distroless), Benutzer 10001, Gruppe 999. Ein Eindringling
# findet im Container kein Werkzeug, mit dem er weiterkommt; ein Angriff über
# eine Shell ist ausgeschlossen, weil es keine gibt.
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
ENV QUIVER_ADRESSE=0.0.0.0:8740 \
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
