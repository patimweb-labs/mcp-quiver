<script setup lang="ts">
// Kopf der Verwaltung: Wortmarke links, Farbschema und Abmelden rechts.
// Dasselbe Muster wie `bausteine::kopf` im Client-Fenster: feste Höhe aus
// `size.kopf`, Inhalt mittig, 16 px Rand links und rechts (AGENTS.md §2.11).
import { LogOut } from "lucide-vue-next";
import { WpButton } from "@weppelmann/components";

import type { Thema } from "../api/typen";
import { THEMEN, THEMA_TEXT } from "../logik/thema";
import Marke from "./Marke.vue";
import Segmentauswahl from "./Segmentauswahl.vue";

defineProps<{ thema: Thema }>();
const emit = defineEmits<{ thema: [string]; abmelden: [] }>();

const farbschemen = THEMEN.map((wert) => ({
  wert,
  name: THEMA_TEXT[wert],
}));

function waehleThema(wert: string): void {
  emit("thema", wert);
}
</script>

<template>
  <header class="kopf">
    <Marke />
    <div class="kopf__rechts">
      <Segmentauswahl
        :wert="thema"
        :moeglichkeiten="farbschemen"
        label="Farbschema"
        @waehle="waehleThema"
      />
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
}
</style>
