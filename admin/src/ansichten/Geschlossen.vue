<script setup lang="ts">
// Die Ersteinrichtung ist zu (Aufgabe 25): Das Fenster von 15 Minuten nach dem
// Start des Servers ist vorbei. Ohne Root kommt niemand hinein – also bleibt
// nur, den Server neu zu starten. Der Befehl steht da, damit die Seite nicht
// nur bedauert, sondern weiterhilft.
import { RefreshCw } from "lucide-vue-next";
import { WpButton } from "@weppelmann/components";

import Karte from "../bausteine/Karte.vue";

const emit = defineEmits<{ erneut: [] }>();
</script>

<template>
  <Karte
    titel="Die Ersteinrichtung ist geschlossen"
    titel_als="h1"
    unter="Der Server läuft länger als 15 Minuten."
  >
    <div class="geschlossen">
      <p>
        Ein Server ohne Root lässt sich nur direkt nach dem Start einrichten.
        Starte ihn neu und lade diese Seite dann noch einmal.
      </p>
      <p class="geschlossen__befehl">
        <code>docker restart quiver</code>
      </p>
      <p class="geschlossen__hinweis">
        Ohne Container: den Prozess <code>quiver-server</code> beenden und neu
        starten. Ein bereits gesetzter Root bleibt bestehen – dann erscheint
        hier die Anmeldung.
      </p>
      <div class="geschlossen__aktion">
        <WpButton label="Erneut versuchen" :icon="RefreshCw" @press="emit('erneut')" />
      </div>
    </div>
  </Karte>
</template>

<style scoped>
.geschlossen {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
  max-width: calc(var(--wp-space-8) * 9);
}

.geschlossen__befehl {
  padding: var(--wp-space-2) var(--wp-space-3);
  background: var(--wp-color-bg-input);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  font-family: var(--wp-font-family-mono);
  font-size: var(--wp-font-size-sm);
  overflow-wrap: anywhere;
}

.geschlossen__hinweis {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.geschlossen__aktion {
  display: flex;
  justify-content: flex-end;
  padding-top: var(--wp-space-2);
}
</style>
