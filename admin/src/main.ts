import { createApp } from "vue";

import "@weppelmann/components/style.css";
import "./styles/tokens.css";
import "./styles/schriften.css";
import "./styles/grund.css";

import App from "./App.vue";
import Muster from "./ansichten/Muster.vue";
import { messe_breite } from "./logik/breite";

// Die Musterseite gibt es nur im Entwicklungsbau (`/muster`, ohne Anmeldung):
// sie zeigt keine Daten, nur die Bausteine zum Vergleich mit dem Client-
// Fenster. Im gebauten Stand fällt der Zweig weg.
const muster =
  import.meta.env.DEV && window.location.pathname.replace(/\/+$/, "") === "/muster";

// Die Breitenstufe steht am Wurzelelement (`data-breite`): Seitenleiste und
// Kopf richten sich danach (Aufgabe 18). Beide Oberflächen messen gleich.
messe_breite();
window.addEventListener("resize", messe_breite);

createApp(muster ? Muster : App).mount("#app");
