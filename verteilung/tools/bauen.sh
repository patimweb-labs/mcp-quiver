#!/usr/bin/env bash
# Die mitgelieferten Werkzeuge bauen (Aufgabe 35) — dasselbe, was
# `release.yml` vor dem Paketieren aufruft.
#
#   verteilung/tools/bauen.sh
#
# Gebaut wird der Stand aus `verteilung/tools/stand.txt` (fester Commit, nicht
# `main`), aus einem frischen Klon des öffentlichen Repos. Danach liegen die
# Programme in `dist/tools/` und ein Bericht in `dist/TOOLS.txt`:
# Version, Commit, Anzahl, Größe und die Namen.
#
# Gestrippt wird nach dem Kopieren mit `strip -x` (lokale Symbole weg, die
# dynamische Symboltabelle bleibt): das kann cargo zwar auch
# (`CARGO_PROFILE_RELEASE_STRIP`), aber dort scheitert es still, wenn dem
# Toolchain `libLLVM` fehlt — und dann hieße „gestrippt“ nur behauptet. Fehlt
# `strip` auf dem Läufer, steht das im Bericht.
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
stand="$wurzel/verteilung/tools/stand.txt"

lesen() {
    sed -n "s/^$1=//p" "$stand" | head -1
}

repo=$(lesen repo)
commit=$(lesen commit)
version=$(lesen version)
if [ -z "$repo" ] || [ -z "$commit" ] || [ -z "$version" ]; then
    echo "FEHLER: $stand nennt repo, commit und version nicht vollständig" >&2
    exit 1
fi

# Wohin gebaut wird: derselbe Zielordner wie beim Client (dort räumt
# `rust-cache` auf), damit ein Lauf nicht alles neu übersetzt.
zielordner=${CARGO_TARGET_DIR:-$wurzel/target}
case "$zielordner" in
    /*) ;;
    *) zielordner="$wurzel/$zielordner" ;;
esac
bau=${QUIVER_TOOLS_BAU:-$zielordner/werkzeuge}
dist="$wurzel/dist"
ziel="$dist/tools"
zwischen=$(mktemp -d "${TMPDIR:-/tmp}/quiver-tools.XXXXXX")
quelle="$zwischen/quiver-tools"

aufraeumen() {
    rm -rf "$zwischen"
}
trap aufraeumen EXIT

echo "==> Werkzeuge $version aus $repo@${commit:0:12}"

# Nur diesen einen Commit holen: kein Verlauf, keine Zweige, kein `main`.
git init -q "$quelle"
git -C "$quelle" remote add origin "https://github.com/$repo.git"
git -C "$quelle" fetch -q --depth 1 origin "$commit"
git -C "$quelle" checkout -q --detach FETCH_HEAD

echo "==> bauen"
CARGO_TARGET_DIR="$bau" cargo build --release --locked --quiet \
    --manifest-path "$quelle/Cargo.toml"

# Alle Programme mit dem Präfix `quiver-`; die Kisten darunter sind
# Bibliotheken und tauchen hier nicht auf.
rm -rf "$ziel"
mkdir -p "$ziel"
anzahl=0
for datei in "$bau"/release/quiver-*; do
    [ -f "$datei" ] || continue
    case "$datei" in
        *.d | *.rlib | *.so | *.dylib | *.dll | *.pdb) continue ;;
    esac
    install -m755 "$datei" "$ziel/"
    anzahl=$((anzahl + 1))
done

gestrippt=nein
if command -v strip >/dev/null 2>&1; then
    for programm in "$ziel"/*; do
        strip -x "$programm"
    done
    gestrippt=ja
else
    echo "==> kein »strip« auf diesem Läufer: die Programme behalten ihre Symbole"
fi
if [ "$anzahl" -eq 0 ]; then
    echo "FEHLER: kein Programm in $bau/release — hat der Bau geklappt?" >&2
    exit 1
fi

groesse=$(du -sk "$ziel" | cut -f1)
echo "==> $anzahl Programme, $((groesse / 1024)) MiB"

# Der Bericht: was im Paket steckt, für das Release und zum Nachlesen.
{
    echo "# Werkzeuge im Paket (Aufgabe 35)"
    echo "quelle=https://github.com/$repo"
    echo "commit=$commit"
    echo "version=$version"
    echo "anzahl=$anzahl"
    echo "groesse_kib=$groesse"
    echo "gestrippt=$gestrippt"
    for programm in "$ziel"/*; do
        echo "datei=$(basename "$programm")"
    done
} >"$dist/TOOLS.txt"

ls -l "$ziel"
