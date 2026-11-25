#!/usr/bin/env bash
# Die Programme des Servers aus dem festgelegten Release von quiver-tools
# (Aufgabe 37/38). Läuft im Bau des Abbilds (Stufe `programme`) und lokal in
# einem Temp-Ordner:
#
#   QUIVER_TOOLS_VERSION=0.1.0 verteilung/programme.sh
#   QUIVER_TOOLS_VERSION=0.1.0 QUIVER_TOOLS_QUELLE=/tmp/release \
#     ZIEL=/tmp/programme verteilung/programme.sh
#
# Erwartet werden die Archive des Releases
#
#   quiver-tools-<fassung>-<system>-<arch>.tar.gz    (x86_64 | aarch64)
#
# und eine `SHA256SUMS` daneben. Die Ordner im Abbild heißen weiter
# `<system>/x64|arm64` – so fragt der Client.
#
# **Erst prüfen, dann entpacken.** Jedes erwartete Archiv muss in `SHA256SUMS`
# stehen und stimmen, bevor irgendetwas entpackt wird; danach wird der Inhalt
# gegen die `LISTE.json` des Archivs geprüft (Name, Größe, SHA-256, System und
# Architektur). Fehlt ein Archiv, stimmt eine Prüfsumme nicht oder fehlt ein
# Programm, bricht der Bau ab – lieber kein Abbild als ein halbes.
#
# `QUIVER_TOOLS_VERSION` leer heißt: das Abbild wird **ohne** Programme gebaut,
# und der Bau sagt es. Nützlich für eine Bibliothek ohne Werkzeuge.
set -euo pipefail

version=${QUIVER_TOOLS_VERSION:-}
ziel=${ZIEL:-/programme}
# Die Archive, die das Release baut: System und Architektur wie im Dateinamen.
archive=${QUIVER_TOOLS_ARCHIVE:-"windows-x86_64 macos-aarch64 macos-x86_64 linux-x86_64 linux-aarch64"}
basis=${QUIVER_TOOLS_BASIS:-https://github.com/patimweb-labs/quiver-tools/releases/download}
# Ein örtlicher Ordner statt des Netzes (für das Durchspielen im Temp-Ordner).
ortlich=${QUIVER_TOOLS_QUELLE:-}

fehler() {
    echo "FEHLER: $*" >&2
    exit 1
}

if [ -z "$version" ]; then
    echo "=============================================================="
    echo "HINWEIS: QUIVER_TOOLS_VERSION ist leer."
    echo "Dieses Abbild enthaelt KEINE Werkzeug-Programme. Geraete bekommen"
    echo "beim Abgleich nichts geladen (die Konfiguration bleibt)."
    echo "Zum Mitliefern: --build-arg QUIVER_TOOLS_VERSION=<fassung>"
    echo "=============================================================="
    mkdir -p "$ziel"
    exit 0
fi

arbeit=$(mktemp -d)
trap 'rm -rf "$arbeit"' EXIT

# Ein Archiv holen: aus dem örtlichen Ordner oder aus dem Release.
holen() {
    # holen <dateiname> <ziel>
    if [ -n "$ortlich" ]; then
        cp "$ortlich/$1" "$2" || fehler "»$1« fehlt in $ortlich"
    else
        curl -fsSL --retry 3 -o "$2" "$basis/v$version/$1" ||
            fehler "»$1« nicht abholbar ($basis/v$version)"
    fi
}

# Die SHA-256 einer Datei – `sha256sum` (Linux) oder `shasum` (macOS).
summe() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        fehler "weder sha256sum noch shasum gefunden"
    fi
}

# `x86_64`/`aarch64` (Archiv) → `x64`/`arm64` (Ordner im Abbild).
ordner_arch() {
    case "$1" in
        x86_64) echo x64 ;;
        aarch64) echo arm64 ;;
        *) fehler "unbekannte Architektur »$1« im Archivnamen" ;;
    esac
}

echo "==> Programme aus quiver-tools $version"

# ---------------------------------------------------------------- 1. Prüfen
holen SHA256SUMS "$arbeit/SHA256SUMS"
for eintrag in $archive; do
    datei="quiver-tools-$version-$eintrag.tar.gz"
    holen "$datei" "$arbeit/$datei"
    # Steht das Archiv in der Liste? (`sha256sum` schreibt zwei Leerzeichen,
    # `shasum` eines; ein `*` vor dem Namen heißt Binärmodus.)
    erwartet=$(awk -v datei="$datei" '
        { name = $2; sub(/^\*/, "", name) }
        name == datei { print $1; exit }
    ' "$arbeit/SHA256SUMS")
    [ -n "$erwartet" ] || fehler "»$datei« steht nicht in SHA256SUMS"
    ist=$(summe "$arbeit/$datei")
    [ "$ist" = "$erwartet" ] ||
        fehler "Prüfsumme von »$datei« stimmt nicht (erwartet $erwartet, ist $ist)"
done
echo "==> alle Archive stimmen mit SHA256SUMS"

# ---------------------------------------------------------------- 2. Entpacken
for eintrag in $archive; do
    datei="quiver-tools-$version-$eintrag.tar.gz"
    system=${eintrag%-*}
    archiv_arch=${eintrag##*-}
    ordner=$(ordner_arch "$archiv_arch")
    entpackt="$arbeit/entpackt-$eintrag"
    mkdir -p "$entpackt"
    tar -xzf "$arbeit/$datei" -C "$entpackt"
    # Bringt ein Archiv einen Ordner mit, zählt dessen Inhalt.
    inhalt="$entpackt"
    if [ ! -f "$entpackt/LISTE.json" ]; then
        inhalt=$(find "$entpackt" -mindepth 1 -maxdepth 1 -type d | head -1)
    fi
    liste="$inhalt/LISTE.json"
    [ -f "$liste" ] || fehler "»$datei« bringt keine LISTE.json"

    # System und Architektur müssen zum Dateinamen passen – sonst liegt ein
    # Archiv unter dem falschen Namen.
    laut_system=$(sed -n 's/.*"system": *"\([^"]*\)".*/\1/p' "$liste" | head -1)
    laut_arch=$(sed -n 's/.*"arch": *"\([^"]*\)".*/\1/p' "$liste" | head -1)
    [ "$laut_system" = "$system" ] ||
        fehler "»$datei« nennt System »$laut_system« statt »$system«"
    [ "$laut_arch" = "$archiv_arch" ] ||
        fehler "»$datei« nennt Architektur »$laut_arch« statt »$archiv_arch«"

    # Je Programm: da, richtige Größe, richtige Prüfsumme.
    sed -n 's/.*"name": *"\([^"]*\)".*"groesse": *\([0-9][0-9]*\).*"sha256": *"\([0-9a-f][0-9a-f]*\)".*/\1 \2 \3/p' \
        "$liste" >"$arbeit/eintraege"
    [ -s "$arbeit/eintraege" ] || fehler "»$datei« nennt kein Programm in LISTE.json"
    while read -r name groesse sha; do
        programm="$inhalt/$name"
        if [ -f "$inhalt/$name.exe" ]; then
            programm="$inhalt/$name.exe"
        fi
        [ -f "$programm" ] || fehler "»$datei« bringt »$name« nicht mit"
        ist_groesse=$(wc -c <"$programm" | tr -d ' ')
        [ "$ist_groesse" = "$groesse" ] ||
            fehler "»$name« in »$datei« ist $ist_groesse statt $groesse Bytes groß"
        ist_summe=$(summe "$programm")
        [ "$ist_summe" = "$sha" ] ||
            fehler "Prüfsumme von »$name« in »$datei« stimmt nicht mit LISTE.json"
    done <"$arbeit/eintraege"

    mkdir -p "$ziel/$system/$ordner"
    cp -a "$inhalt/." "$ziel/$system/$ordner/"
    echo "==> $system/$ordner aus $datei"
done

echo "==> Programme aus quiver-tools $version liegen bereit:"
find "$ziel" -type f | sort
