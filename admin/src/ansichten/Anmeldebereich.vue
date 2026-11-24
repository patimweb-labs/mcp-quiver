<script setup lang="ts">
// Der Rahmen vor der Anmeldung. Er zeigt genau eine Lage (AGENTS.md §2.3):
// laden, einrichten, geschlossen, anmelden, keine Rolle oder Fehler. Die
// Entscheidung, welche Lage gilt, fällt in `logik/anmeldung.ts` – hier steht
// nur, wie sie aussieht.
import { computed } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { WpButton, WpSpinner } from "@weppelmann/components";

import type { Ich, Thema } from "../api/typen";
import type { Lage } from "../logik/anmeldung";
import { anzeige_name, rollen_text } from "../logik/anmeldung";
import { THEMEN, THEMA_TEXT } from "../logik/thema";
import Kasten from "../bausteine/Kasten.vue";
import Karte from "../bausteine/Karte.vue";
import Marke from "../bausteine/Marke.vue";
import Segmentauswahl from "../bausteine/Segmentauswahl.vue";
import Anmeldung from "./Anmeldung.vue";
import Einrichtung from "./Einrichtung.vue";
import Geschlossen from "./Geschlossen.vue";

const props = defineProps<{ lage: Lage; thema: Thema }>();
const emit = defineEmits<{
  thema: [wert: string];
  angemeldet: [ich: Ich];
  erneut: [];
  abmelden: [];
}>();

const farbschemen = THEMEN.map((wert) => ({
  wert,
  name: THEMA_TEXT[wert],
}));
/** Wer nicht verwalten darf, erfährt hier, wer er ist. */
const kennung = computed(() =>
  props.lage.art === "kein_zugang"
    ? `${anzeige_name(props.lage.ich.benutzer)} · ${rollen_text(props.lage.ich.benutzer.rolle)}`
    : "",
);
</script>

<template>
  <main class="bereich">
    <header class="bereich__kopf">
      <Marke :thema="thema" />
      <div class="bereich__rechts">
        <Segmentauswahl
          :wert="thema"
          :moeglichkeiten="farbschemen"
          label="Farbschema"
          @waehle="(wert) => emit('thema', wert)"
        />
      </div>
    </header>

    <div class="bereich__breite">
      <!-- Der Stand des Servers wird geholt. -->
      <Kasten v-if="lage.art === 'laedt'">
        <p class="bereich__laedt" role="status">
          <WpSpinner size="sm" label="Lädt" />
          <span>Einen Moment …</span>
        </p>
      </Kasten>

      <Einrichtung
        v-else-if="lage.art === 'einrichtung'"
        :code_noetig="lage.code_noetig"
        @angemeldet="(ich) => emit('angemeldet', ich)"
      />

      <Geschlossen v-else-if="lage.art === 'geschlossen'" @erneut="emit('erneut')" />

      <Anmeldung
        v-else-if="lage.art === 'anmeldung'"
        :entra="lage.entra"
        :hinweis="lage.hinweis"
        @angemeldet="(ich) => emit('angemeldet', ich)"
      />

      <Karte
        v-else-if="lage.art === 'kein_zugang'"
        titel="Keine Rolle für die Verwaltung"
        titel_als="h1"
        :unter="kennung"
      >
        <p>
          Angemeldet ist ein Konto ohne Verwaltungsrecht. Ein Admin vergibt die
          Rolle – danach hilft ein Neuladen.
        </p>
        <div class="bereich__aktion">
          <WpButton label="Abmelden" @press="emit('abmelden')" />
        </div>
      </Karte>

      <Karte
        v-else
        titel="Der Server antwortet nicht"
        titel_als="h1"
        :unter="lage.art === 'fehler' ? lage.text : ''"
      >
        <p>
          Die Verwaltung konnte den Stand des Servers nicht holen. Adresse und
          Verbindung prüfen, dann erneut versuchen.
        </p>
        <div class="bereich__aktion">
          <WpButton
            label="Erneut versuchen"
            :icon="RefreshCw"
            variant="primary"
            @press="emit('erneut')"
          />
        </div>
      </Karte>
    </div>
  </main>
</template>

<style scoped>
.bereich {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-5);
  min-height: 100%;
  padding: var(--wp-space-4);
  background: var(--wp-color-bg-canvas);
}

.bereich__kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-3);
  flex-wrap: wrap;
}

.bereich__rechts {
  display: flex;
  align-items: center;
  gap: var(--wp-space-3);
  min-width: 0;
}

/* Die Formulare bleiben schmal und mittig, auch auf großen Fenstern. */
.bereich__breite {
  width: min(calc(var(--wp-space-8) * 8), 100%);
  margin-inline: auto;
  padding-bottom: var(--wp-space-6);
}

.bereich__laedt {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  color: var(--wp-color-text-muted);
}

.bereich__aktion {
  display: flex;
  justify-content: flex-end;
  padding-top: var(--wp-space-2);
}
</style>
