<script setup lang="ts">
// Kopf der Verwaltung: Wortmarke links, Theme-Auswahl und Abmelden rechts.
// Entspricht dem Kopf des Client-Fensters (bausteine::marke, thema::auswahl).
import { LogOut } from "lucide-vue-next";
import { WpButton, WpToggleGroup } from "@weppelmann/components";

import type { Thema } from "../api/typen";
import { THEMEN, THEMA_TEXT } from "../logik/thema";
import Marke from "./Marke.vue";

defineProps<{ thema: Thema }>();
const emit = defineEmits<{ thema: [string]; abmelden: [] }>();

const farbschemen = THEMEN.map((wert) => ({
  value: wert,
  label: THEMA_TEXT[wert],
}));

function waehleThema(wert: string): void {
  emit("thema", wert);
}
</script>

<template>
  <header class="kopf">
    <Marke />
    <div class="kopf__rechts">
      <WpToggleGroup
        :model-value="thema"
        :options="farbschemen"
        size="sm"
        label="Farbschema"
        @update:model-value="waehleThema"
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
  flex-wrap: wrap;
  padding: var(--wp-space-2) var(--wp-space-4);
  background: var(--wp-color-bg-panel);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.kopf__rechts {
  display: flex;
  align-items: center;
  gap: var(--wp-space-3);
  flex-wrap: wrap;
}
</style>
