#!/bin/sh
# Prüft das Abbild so, wie es im Betrieb läuft (Härtung, Punkt 7):
# genau ein Container mit `--read-only`, `--tmpfs /tmp`, `--cap-drop=ALL`,
# `--security-opt=no-new-privileges`, `--memory`, `--pids-limit`, eigener
# Log-Rotation, eigenem Volume, freiem Port und eigenem Namen.
#
# Der Container heißt `quiver-pruefung-*`, sein Volume auch; am Ende wird beides
# entfernt. Der laufende Container `quiver` (Port 8740) wird **nicht** angefasst.
#
#   tools/container-pruefung.sh              # baut das Abbild und prüft
#   tools/container-pruefung.sh --ohne-bau   # nimmt das vorhandene Abbild
#
# Was geprüft wird:
#   1. Die Grenzen aus dem Aufruf sind wirklich gesetzt (`docker inspect`).
#   2. Der Container wird „healthy“ – der HEALTHCHECK läuft über den
#      Unterbefehl `quiver-server gesund` (nur Loopback).
#   3. `docker exec` funktioniert ohne Shell (die Binärdatei liegt im PATH).
#   4. Ein zweiter Start auf demselben Volume funktioniert ohne Handarbeit
#      (UID und GID bleiben 10001:999, die Datenbank bleibt liegen).
set -eu

BILD="${QUIVER_PRUEFUNG_BILD:-mcp-quiver-pruefung:lokal}"
NAME="quiver-pruefung-$$"
VOLUME="quiver-pruefung-daten-$$"
# Ein freier Port außerhalb der belegten Bereiche (8740–8742 bleiben tabu).
PORT="${QUIVER_PRUEFUNG_PORT:-18740}"

aufraeumen() {
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker volume rm "$VOLUME" >/dev/null 2>&1 || true
}
trap aufraeumen EXIT INT TERM

pruefe() {
    # pruefe <beschreibung> <erwartet> <ist>
    if [ "$2" != "$3" ]; then
        echo "FEHLER: $1 ist »$3«, erwartet »$2«"
        exit 1
    fi
    echo "   $1: $3"
}

warten_auf_gesundheit() {
    i=0
    while [ "$i" -lt 45 ]; do
        status=$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}kein-healthcheck{{end}}' "$NAME")
        [ "$status" = "healthy" ] && { echo "   healthy"; return 0; }
        if [ "$status" = "unhealthy" ] || [ "$status" = "kein-healthcheck" ]; then
            echo "FEHLER: Status $status"
            docker logs "$NAME" 2>&1 | tail -20
            return 1
        fi
        i=$((i + 1))
        sleep 2
    done
    echo "FEHLER: nach 90 s nicht healthy (Status $status)"
    docker logs "$NAME" 2>&1 | tail -20
    return 1
}

starten() {
    docker run -d --name "$NAME" \
        --read-only \
        --tmpfs /tmp:size=32m,mode=1777 \
        --cap-drop=ALL \
        --security-opt=no-new-privileges:true \
        --memory 512m \
        --pids-limit 256 \
        --log-driver json-file --log-opt max-size=10m --log-opt max-file=3 \
        -p "127.0.0.1:$PORT:8740" \
        -v "$VOLUME:/daten" \
        "$BILD" >/dev/null
}

if [ "${1:-}" != "--ohne-bau" ]; then
    echo "== Abbild bauen: $BILD"
    docker build -t "$BILD" .
fi

echo "== Starten: $NAME auf 127.0.0.1:$PORT mit Volume $VOLUME"
starten

echo "== Grenzen des Containers"
pruefe "read-only" "true" "$(docker inspect -f '{{.HostConfig.ReadonlyRootfs}}' "$NAME")"
pruefe "tmpfs /tmp" "/tmp:size=32m,mode=1777" "$(docker inspect -f '{{range $ziel, $wert := .HostConfig.Tmpfs}}{{$ziel}}:{{$wert}}{{end}}' "$NAME")"
pruefe "cap-drop" "[ALL]" "$(docker inspect -f '{{.HostConfig.CapDrop}}' "$NAME")"
pruefe "no-new-privileges" "[no-new-privileges:true]" "$(docker inspect -f '{{.HostConfig.SecurityOpt}}' "$NAME")"
pruefe "memory" "536870912" "$(docker inspect -f '{{.HostConfig.Memory}}' "$NAME")"
pruefe "pids-limit" "256" "$(docker inspect -f '{{.HostConfig.PidsLimit}}' "$NAME")"
pruefe "log rotation" "10m" "$(docker inspect -f '{{index .HostConfig.LogConfig.Config "max-size"}}' "$NAME")"
pruefe "Benutzer" "10001:999" "$(docker inspect -f '{{.Config.User}}' "$NAME")"

echo "== HEALTHCHECK über den Unterbefehl"
warten_auf_gesundheit

echo "== docker exec ohne Shell"
docker exec "$NAME" quiver-server benutzer >/dev/null
echo "   quiver-server benutzer läuft"
docker exec "$NAME" quiver-server gesund >/dev/null
echo "   quiver-server gesund meldet: in Ordnung"

echo "== Zweiter Start auf demselben Volume"
docker rm -f "$NAME" >/dev/null
starten
warten_auf_gesundheit
docker exec "$NAME" quiver-server benutzer >/dev/null
echo "   vorhandenes Volume funktioniert ohne Handarbeit"

echo "== Volume"
pruefe "Volume am Datenordner" "$VOLUME" \
    "$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/daten"}}{{.Name}}{{end}}{{end}}' "$NAME")"

echo "== SIGTERM (docker stop) endet mit 0"
# Der Server ist im Container PID 1 und muss SIGTERM selbst behandeln; sonst
# endet der Container nach der Frist hart mit 137.
docker stop -t 10 "$NAME" >/dev/null
sleep 1
pruefe "Exit-Code nach docker stop" "0" "$(docker inspect -f '{{.State.ExitCode}}' "$NAME")"

echo
echo "Alles in Ordnung. $NAME und $VOLUME werden entfernt."
