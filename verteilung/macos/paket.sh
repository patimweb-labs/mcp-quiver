#!/usr/bin/env bash
# macOS-Paket: `quiver.app` und ein `.dmg` — dieselbe Datei, die die CI baut
# und die am Release hängt.
#
#   verteilung/macos/paket.sh                 # baut, ad hoc signiert
#   MACOS_CERTIFICATE=… MACOS_SIGNING_IDENTITY=… APPLE_ID=… \
#     verteilung/macos/paket.sh               # signiert und notarisiert
#
# Ergebnisse:
#   dist/quiver.app                                    (zum Ausprobieren)
#   dist/quiver-<version>-macos-<arch>.dmg
#   dist/SIGNIERT.txt                                  (signiert-und-notarisiert,
#                                                       signiert, unsigniert)
#
# Ohne Geheimnisse wird **ad hoc** signiert (`codesign -s -`): das Bündel läuft
# damit auf dem eigenen Rechner, aber Gatekeeper fragt beim ersten Start nach
# einer Bestätigung. Was das heißt, steht in `docs/release.de.md`.
#
# Der Schlüsselbund, das entschlüsselte Zertifikat und die geänderte
# Schlüsselbund-Liste werden am Ende wieder abgeräumt — auch wenn etwas
# schiefgeht (`trap … EXIT`).
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$wurzel"

version=$("$wurzel/verteilung/version.sh")
arch=${ARCH:-$(uname -m)}
case "$arch" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64) arch=x86_64 ;;
    *)
        echo "FEHLER: unbekannte Architektur »$arch« (erwartet arm64 oder x86_64)" >&2
        exit 1
        ;;
esac

dist="$wurzel/dist"
app="$dist/quiver.app"
# `CARGO_TARGET_DIR` gilt auch hier: sonst suchte das Skript in `target/`,
# gebaut hätte es woanders hin.
zielordner=${CARGO_TARGET_DIR:-$wurzel/target}
case "$zielordner" in
    /*) ;;
    *) zielordner="$wurzel/$zielordner" ;;
esac
dmg="$dist/quiver-$version-macos-$arch.dmg"
zwischen=$(mktemp -d "${TMPDIR:-/tmp}/quiver-macos.XXXXXX")

schluesselbund=""
zertifikat=""
alte_schluesselbuende=""

aufraeumen() {
    set +e
    if [ -n "$schluesselbund" ]; then
        # Erst die Liste zurücksetzen, dann den Schlüsselbund löschen: sonst
        # bleibt ein gelöschter Eintrag in der Liste des Benutzers stehen.
        if [ -n "$alte_schluesselbuende" ]; then
            # shellcheck disable=SC2086 # die Liste ist absichtlich wortweise
            security list-keychains -d user -s $alte_schluesselbuende >/dev/null 2>&1
        fi
        security delete-keychain "$schluesselbund" >/dev/null 2>&1
    fi
    [ -n "$zertifikat" ] && rm -f "$zertifikat"
    rm -rf "$zwischen"
}
trap aufraeumen EXIT

echo "==> quiver $version für macOS $arch"

# ---------------------------------------------------------------- Bauen
# Nativ auf dem Läufer, kein Cross-Compile: `MACOSX_DEPLOYMENT_TARGET` steht in
# `release.yml` und gilt damit auch für alle Kisten, die eframe mitbringt.
cargo build --release --locked -p quiver-fenster
programm="$zielordner/release/quiver"
if [ ! -x "$programm" ]; then
    echo "FEHLER: $programm fehlt" >&2
    exit 1
fi

# ---------------------------------------------------------------- Bündel
rm -rf "$app" "$dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m755 "$programm" "$app/Contents/MacOS/quiver"
install -m644 LICENSE "$app/Contents/Resources/LICENSE"
sed "s|__VERSION__|$version|g" \
    "$wurzel/verteilung/macos/Info.plist.in" >"$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Das Symbol entsteht aus den vorhandenen Bildern in `assets/marke` — kein
# neues Logo. `quiver-dock.png` ist das Bild, das der Client selbst als
# Fenster-Symbol nimmt (`crates/fenster/src/main.rs`), und hat den Rand, den
# Apples Raster verlangt.
iconset="$zwischen/AppIcon.iconset"
mkdir -p "$iconset"
for kante in 16 32 64 128 256 512; do
    sips -z "$kante" "$kante" assets/marke/quiver-dock.png \
        --out "$iconset/icon_${kante}x${kante}.png" >/dev/null
    doppelt=$((kante * 2))
    sips -z "$doppelt" "$doppelt" assets/marke/quiver-dock.png \
        --out "$iconset/icon_${kante}x${kante}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

# ---------------------------------------------------------------- Werkzeuge
# Die Programme aus quiver-tools liegen neben dem Programm im Bündel; ein
# Eintrag der Bibliothek nennt dann nur noch den Namen (Aufgabe 35).
"$wurzel/verteilung/tools/einbauen.sh" "$dist/tools" \
    "$app/Contents/Resources/tools"

# ---------------------------------------------------------------- Signieren
# Die Werkzeuge liegen im Bündel und werden **einzeln** signiert, bevor das
# Bündel selbst drankommt: die Signatur des Bündels versiegelt auch seinen
# Inhalt, später signieren macht sie ungültig (Aufgabe 35).
"$wurzel/verteilung/tools/macos-signieren.sh" "$app" \
    "${MACOS_SIGNING_IDENTITY:-}"

zustand=unsigniert
if [ -n "${MACOS_CERTIFICATE:-}" ] && [ -n "${MACOS_SIGNING_IDENTITY:-}" ]; then
    echo "==> signiere mit »$MACOS_SIGNING_IDENTITY«"
    schluesselbund="$zwischen/quiver-signatur.keychain-db"
    zertifikat="$zwischen/zertifikat.p12"
    passwort="${MACOS_CERTIFICATE_PASSWORD:-}"
    printf '%s' "$MACOS_CERTIFICATE" | openssl base64 -d -A -out "$zertifikat"
    security create-keychain -p "$passwort" "$schluesselbund"
    security set-keychain-settings -lut 3600 "$schluesselbund"
    security unlock-keychain -p "$passwort" "$schluesselbund"
    security import "$zertifikat" -k "$schluesselbund" -P "$passwort" \
        -T /usr/bin/codesign >/dev/null
    security set-key-partition-list -S apple-tool:,apple: -s -k "$passwort" \
        "$schluesselbund" >/dev/null
    alte_schluesselbuende=$(security list-keychains -d user | tr -d '"' | tr '\n' ' ')
    # shellcheck disable=SC2086 # die Liste ist absichtlich wortweise
    security list-keychains -d user -s "$schluesselbund" $alte_schluesselbuende
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_SIGNING_IDENTITY" "$app"
    zustand=signiert
else
    echo "==> keine Geheimnisse gesetzt: ad hoc signiert (Gatekeeper fragt)"
    codesign --force --sign - "$app"
fi
codesign --verify --strict "$app"

# ---------------------------------------------------------------- Datenträger
buehne="$zwischen/dmg"
mkdir -p "$buehne"
cp -R "$app" "$buehne/"
ln -s /Applications "$buehne/Applications"
hdiutil create -quiet -volname "mcp-quiver" -srcfolder "$buehne" -ov \
    -format UDZO "$dmg"

if [ "$zustand" = signiert ] && [ -n "${APPLE_ID:-}" ] &&
    [ -n "${APPLE_APP_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    echo "==> notarisiere"
    codesign --force --timestamp --sign "$MACOS_SIGNING_IDENTITY" "$dmg"
    xcrun notarytool submit "$dmg" \
        --apple-id "$APPLE_ID" \
        --team-id "$APPLE_TEAM_ID" \
        --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$dmg"
    zustand=signiert-und-notarisiert
fi

printf '%s\n' "$zustand" >"$dist/SIGNIERT.txt"
echo "==> $dmg ($zustand)"
ls -la "$dist"
