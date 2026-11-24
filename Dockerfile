# mcp-quiver-server: ein Container, ein Port (8740), ein Volume (/daten).
# Baut die Admin-Oberfläche (Vue) und den Server (Rust) und liefert beides aus.
#
# Basis-Images kommen von Docker Hub. Ohne Docker-Hub-Login geht der öffentliche Spiegel:
#   docker build --build-arg BASIS=public.ecr.aws/docker/library -t mcp-quiver-server .
ARG BASIS=docker.io/library

# --- Admin-Oberfläche ---------------------------------------------------------
FROM ${BASIS}/node:22-bookworm-slim AS admin
ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
WORKDIR /src
# Die Oberfläche braucht die Tokens und Schriften aus dem Rust-Teil (eine Quelle).
COPY tokens tokens
COPY crates/oberflaeche/schriften crates/oberflaeche/schriften
COPY admin admin
WORKDIR /src/admin
RUN npm ci --no-audit --no-fund && npm run build

# --- Server -------------------------------------------------------------------
FROM ${BASIS}/rust:1-slim-bookworm AS server
WORKDIR /src
# rust-toolchain.toml legt die Version fest; rustup holt sie hier.
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates crates
COPY tokens tokens
# Die Marke der Seiten (das Original liegt in `assets/marke`).
COPY assets assets
RUN cargo build --release --locked -p quiver-server \
 && cp target/release/quiver-server /quiver-server

# --- Laufzeit -----------------------------------------------------------------
FROM ${BASIS}/debian:bookworm-slim
RUN useradd --system --uid 10001 --home /daten quiver \
 && mkdir -p /daten && chown quiver /daten
COPY --from=server /quiver-server /usr/local/bin/quiver-server
COPY --from=admin /src/admin/dist /opt/quiver/admin
ENV QUIVER_ADRESSE=0.0.0.0:8740 \
    QUIVER_DATEN=/daten \
    QUIVER_ADMIN_DIST=/opt/quiver/admin \
    RUST_LOG=info
USER quiver
VOLUME /daten
EXPOSE 8740
ENTRYPOINT ["/usr/local/bin/quiver-server"]
