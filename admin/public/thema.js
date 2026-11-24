// Das Theme steht vor dem ersten Bild fest, sonst blitzt das dunkle
// Standard-Theme kurz auf. Drei Themes und nichts sonst (AGENTS.md §2.7).
//
// Eine eigene Datei und kein `<script>` im HTML: die Sicherheitsköpfe des
// Servers erlauben kein Inline-JavaScript (`script-src 'self'`, Aufgabe 20).
// Als klassisches Skript im Kopf läuft es vor dem ersten Aufbau der Seite.
try {
  document.documentElement.dataset.theme =
    localStorage.getItem("quiver-thema") || "dark";
} catch (fehler) {
  // Privater Modus oder gesperrter Speicher: dann eben das Standard-Theme.
  document.documentElement.dataset.theme = "dark";
}
