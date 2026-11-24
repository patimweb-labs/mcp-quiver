<script setup lang="ts">
// Kopf der Verwaltung: Wortmarke links, Farbschema, Name mit Rolle und
// Abmelden rechts.
// Dasselbe Muster wie `bausteine::kopf` im Client-Fenster: feste Höhe aus
// `size.kopf`, Inhalt mittig, 16 px Rand links und rechts (AGENTS.md §2.11).
//
// Unter `breakpoint.formular` ist für die Segmentreihe kein Platz: dann öffnet
// ein Zeichen das Farbschema als Menü — wie im Client-Fenster (Aufgabe 18).
import { computed } from "vue";
import { LogOut, Settings } from "lucide-vue-next";
import { WpButton, WpIconButton, WpMenu } from "@weppelmann/components";
import type { MenuItem } from "@weppelmann/components";

import type { Ich, Thema } from "../api/typen";
import { breite } from "../logik/breite";
import { anzeige_name, rollen_text } from "../logik/anmeldung";
import { THEMEN, THEMA_TEXT } from "../logik/thema";
import Marke from "./Marke.vue";
import Segmentauswahl from "./Segmentauswahl.vue";

// `ich` fehlt nur auf der Musterseite: sie vergleicht Maße, sie zeigt keinen
// Benutzer (dort steht dieselbe Kopfzeile wie im Client-Fenster).
const props = defineProps<{ thema: Thema; ich?: Ich }>();
const emit = defineEmits<{ thema: [string]; abmelden: [] }>();

const name = computed(() => (props.ich ? anzeige_name(props.ich.benutzer) : ""));
const rolle = computed(() => (props.ich ? rollen_text(props.ich.benutzer.rolle) : ""));

const farbschemen = THEMEN.map((wert) => ({
  wert,
  name: THEMA_TEXT[wert],
}));

/** Das Menü der schmalen Kopfzeile; das gewählte Farbschema trägt den Haken. */
const eintraege = computed<MenuItem[]>(() =>
  THEMEN.map((wert) => ({
    id: wert,
    label: THEMA_TEXT[wert],
    hint: wert === props.thema ? "✓" : undefined,
  })),
);

function waehleThema(wert: string): void {
  emit("thema", wert);
}
</script>

<template>
  <header class="kopf">
    <Marke :thema="thema" />
    <div class="kopf__rechts">
      <Segmentauswahl
        v-if="breite !== 'eng'"
        :wert="thema"
        :moeglichkeiten="farbschemen"
        label="Farbschema"
        @waehle="waehleThema"
      />
      <WpMenu
        v-else
        :items="eintraege"
        label="Farbschema"
        @select="waehleThema"
      >
        <WpIconButton :icon="Settings" label="Farbschema" size="sm" />
      </WpMenu>
      <span v-if="ich" class="kopf__ich" :title="`${name} · ${rolle}`">
        <span class="kopf__name">{{ name }}</span>
        <span class="kopf__rolle">{{ rolle }}</span>
      </span>
      <WpButton
        label="Abmelden"
        variant="secondary"
        size="sm"
        :icon="LogOut"
        @press="emit('abmelden')"
      />
    </div>
  </header>
</template>

<style scoped>
.kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-3);
  height: var(--wp-size-kopf);
  padding-inline: var(--wp-space-4);
  background: var(--wp-color-bg-panel);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.kopf__rechts {
  display: flex;
  align-items: center;
  gap: var(--wp-space-3);
  min-width: 0;
}

/* Name und Rolle: der Name darf schrumpfen, die Rolle bleibt lesbar. */
.kopf__ich {
  display: flex;
  align-items: baseline;
  gap: var(--wp-space-2);
  min-width: 0;
}

.kopf__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: calc(var(--wp-space-8) * 3);
}

.kopf__rolle {
  flex: none;
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}
</style>
