#!/usr/bin/env bash
# Linux-Pakete: `.tar.gz`, `.deb`, `.rpm` und AppImage — dieselben Dateien, die
# die CI baut und die am Release hängen.
#
#   verteilung/linux/paket.sh
#   ARCH=aarch64 verteilung/linux/paket.sh     # ohne eigenes aarch64-System
#
# Ergebnisse in `dist/`:
#   quiver-<version>-linux-<arch>.tar.gz
#   mcp-quiver_<version>_<amd64|arm64>.deb
#   mcp-quiver-<version>.<x86_64|aarch64>.rpm
#   quiver-<version>-linux-<x86_64|aarch64>.AppImage
#
# Werkzeuge (nfpm, appimagetool) werden bei Bedarf nach `dist/werkzeuge/`
# geladen; beide mit fester Version und fester Prüfsumme.
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$wurzel"

NFPM_VERSION=2.43.0
NFPM_SHA_X86_64=a80d5f724ed70b192ffa8a2bde469c013cef559c8afa3441eb51dd9a918beb6b
NFPM_SHA_AARCH64=cfafa3bee852c45a52f2ebbfb063d715235f934d082dc64301a1b9957f3a4032
APPIMAGETOOL_VERSION=1.9.1
APPIMAGETOOL_SHA_X86_64=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
APPIMAGETOOL_SHA_AARCH64=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158

# nfpm rechnet selbst um: `amd64` wird für .rpm zu `x86_64`, `arm64` zu
# `aarch64` — deshalb stehen hier die Namen, die nfpm erwartet, nicht die der
# Paketformate.
version=$("$wurzel/verteilung/version.sh")
arch=${ARCH:-$(uname -m)}
case "$arch" in
    x86_64)
        nfpmarch=amd64
        nfpm_sha=$NFPM_SHA_X86_64
        nfpm_datei="nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz"
        appimage_sha=$APPIMAGETOOL_SHA_X86_64
        ;;
    aarch64 | arm64)
        arch=aarch64
        nfpmarch=arm64
        nfpm_sha=$NFPM_SHA_AARCH64
        nfpm_datei="nfpm_${NFPM_VERSION}_Linux_arm64.tar.gz"
        appimage_sha=$APPIMAGETOOL_SHA_AARCH64
        ;;
    *)
        echo "FEHLER: unbekannte Architektur »$arch« (erwartet x86_64 oder aarch64)" >&2
        exit 1
        ;;
esac

dist="$wurzel/dist"
werkzeuge="$dist/werkzeuge"
# `CARGO_TARGET_DIR` gilt auch hier: sonst suchte das Skript in `target/`.
zielordner=${CARGO_TARGET_DIR:-$wurzel/target}
case "$zielordner" in
    /*) ;;
    *) zielordner="$wurzel/$zielordner" ;;
esac
name="quiver-$version-linux-$arch"
baum="$dist/$name"
mkdir -p "$dist" "$werkzeuge"

pruefsumme() {
    # pruefsumme <datei> <erwartet>
    echo "$2  $1" | sha256sum -c --quiet -
}

nfpm_holen() {
    local ziel="$werkzeuge/nfpm"
    if [ -x "$ziel" ]; then
        return
    fi
    echo "==> nfpm $NFPM_VERSION"
    curl -fsSL --retry 3 -o "$werkzeuge/$nfpm_datei" \
        "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/$nfpm_datei"
    pruefsumme "$werkzeuge/$nfpm_datei" "$nfpm_sha"
    tar -C "$werkzeuge" -xzf "$werkzeuge/$nfpm_datei" nfpm
    rm -f "$werkzeuge/$nfpm_datei"
}

appimagetool_holen() {
    local ziel="$werkzeuge/appimagetool"
    if [ -x "$ziel" ]; then
        return
    fi
    echo "==> appimagetool $APPIMAGETOOL_VERSION"
    curl -fsSL --retry 3 -o "$ziel" \
        "https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-${arch}.AppImage"
    pruefsumme "$ziel" "$appimage_sha"
    chmod 755 "$ziel"
}

echo "==> quiver $version für Linux $arch"

# ---------------------------------------------------------------- Bauen
# Das Fenster (egui/winit/glow) lädt Wayland, X11 und OpenGL zur Laufzeit; zum
# Bauen genügen Compiler und pkg-config.
cargo build --release --locked -p quiver-fenster
if [ ! -x "$zielordner/release/quiver" ]; then
    echo "FEHLER: $zielordner/release/quiver fehlt" >&2
    exit 1
fi

# ---------------------------------------------------------------- Baum
# Ein Baum, drei Verpackungen: .tar.gz bleibt wie er ist, .deb/.rpm und das
# AppImage lesen denselben Baum.
rm -rf "$baum"
install -Dm755 "$zielordner/release/quiver" "$baum/bin/quiver"
install -Dm644 verteilung/linux/de.weppelmann.mcp-quiver.desktop \
    "$baum/share/applications/de.weppelmann.mcp-quiver.desktop"
sed -e "s|__VERSION__|$version|g" -e "s|__DATUM__|$(date -u +%F)|g" \
    verteilung/linux/de.weppelmann.mcp-quiver.metainfo.xml.in \
    >"$dist/metainfo.xml"
install -Dm644 "$dist/metainfo.xml" \
    "$baum/share/metainfo/de.weppelmann.mcp-quiver.metainfo.xml"
rm -f "$dist/metainfo.xml"
# Symbole: nur die vorhandenen Bilder aus `assets/marke`, in ihren echten
# Größen — nichts wird hochskaliert.
for kante in 32 64 256; do
    install -Dm644 "assets/marke/quiver-dunkel-$kante.png" \
        "$baum/share/icons/hicolor/${kante}x${kante}/apps/de.weppelmann.mcp-quiver.png"
done
install -m644 LICENSE README.md "$baum/"
tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"

# ---------------------------------------------------------------- .deb, .rpm
nfpm_holen
sed -e "s|__VERSION__|$version|g" \
    -e "s|__ARCH__|$nfpmarch|g" \
    -e "s|__BAUM__|$baum|g" \
    verteilung/linux/nfpm.yaml.in >"$dist/nfpm.yaml"
"$werkzeuge/nfpm" package --config "$dist/nfpm.yaml" --packager deb --target "$dist"
"$werkzeuge/nfpm" package --config "$dist/nfpm.yaml" --packager rpm --target "$dist"

# ---------------------------------------------------------------- AppImage
appimagetool_holen
appdir="$dist/AppDir"
rm -rf "$appdir"
mkdir -p "$appdir/usr"
# Nur, was der Baum wirklich hat: `bin` und `share`. Der Client bringt keine
# Programme mehr mit (Aufgabe 37) – ein `lib` gibt es nicht.
cp -R "$baum/bin" "$baum/share" "$appdir/usr/"
# Der Text von `AppRun` soll genau so in der Datei stehen — deshalb einfache
# Anführungszeichen: `$0`, `$@` und `$(…)` sind für die App gemeint, nicht für
# dieses Skript.
# shellcheck disable=SC2016
printf '#!/bin/sh\nHERE=$(dirname "$(readlink -f "$0")")\nexec "$HERE/usr/bin/quiver" "$@"\n' \
    >"$appdir/AppRun"
chmod 755 "$appdir/AppRun"
cp "$baum/share/applications/de.weppelmann.mcp-quiver.desktop" "$appdir/"
cp "$baum/share/icons/hicolor/256x256/apps/de.weppelmann.mcp-quiver.png" "$appdir/"
# `APPIMAGE_EXTRACT_AND_RUN=1`: appimagetool selbst ist ein AppImage und
# braucht sonst FUSE. `--no-appstream`: appstreamcli ist auf dem Läufer nicht
# installiert; die Metadaten werden davon unabhängig mitgeliefert.
ARCH=$arch APPIMAGE_EXTRACT_AND_RUN=1 "$werkzeuge/appimagetool" \
    --no-appstream "$appdir" "$dist/$name.AppImage"
rm -rf "$appdir"

echo "==> fertig"
ls -la "$dist"
