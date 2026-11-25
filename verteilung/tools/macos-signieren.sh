#!/usr/bin/env bash
# Die mitgelieferten Werkzeuge im Bündel signieren (Aufgabe 35).
#
#   verteilung/tools/macos-signieren.sh quiver.app [Signatur-Identität]
#
# Die Werkzeuge liegen in `quiver.app/Contents/Resources/tools` und sind damit
# Teil des Bündels: sie müssen **einzeln** signiert sein, bevor das Bündel
# selbst drankommt. Sonst ist die Signatur unvollständig (`codesign --verify
# --strict`) und die Notarisierung lehnt das Paket ab.
#
# Ohne Identität wird ad hoc signiert (`-`) — wie das Bündel selbst.
set -euo pipefail

app=${1:?Bündel fehlt}
identitaet=${2:-}
ordner="$app/Contents/Resources/tools"

if [ ! -d "$ordner" ]; then
    exit 0
fi

for werkzeug in "$ordner"/*; do
    [ -f "$werkzeug" ] || continue
    if [ -n "$identitaet" ]; then
        codesign --force --options runtime --timestamp --sign "$identitaet" "$werkzeug"
    else
        codesign --force --sign - "$werkzeug"
    fi
done
echo "==> Werkzeuge in $ordner signiert"
