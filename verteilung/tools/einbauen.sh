#!/usr/bin/env bash
# Die gebauten Werkzeuge in ein Paket legen (Aufgabe 35).
#
#   verteilung/tools/einbauen.sh <quelle> <ziel>
#
# <quelle> ist `dist/tools` (aus `verteilung/tools/bauen.sh`), <ziel> der
# Ordner im Paket. Fehlt die Quelle, bricht der Bau ab: ein Paket ohne
# Werkzeuge kann keinen Eintrag mit bloßem Programmnamen starten.
set -euo pipefail

quelle=${1:?Quelle fehlt (dist/tools)}
ziel=${2:?Ziel fehlt}

if [ ! -d "$quelle" ]; then
    echo "FEHLER: $quelle fehlt — erst »verteilung/tools/bauen.sh« laufen lassen." >&2
    exit 1
fi

mkdir -p "$ziel"
anzahl=0
for programm in "$quelle"/*; do
    [ -f "$programm" ] || continue
    install -m755 "$programm" "$ziel/"
    anzahl=$((anzahl + 1))
done
if [ "$anzahl" -eq 0 ]; then
    echo "FEHLER: $quelle ist leer — erst »verteilung/tools/bauen.sh« laufen lassen." >&2
    exit 1
fi
echo "==> $anzahl Werkzeuge nach $ziel"
