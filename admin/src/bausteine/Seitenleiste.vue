<script setup lang="ts">
// Navigation links. Eine Zeile je Seite, mit Zeichen, Name und – wenn
// vorhanden – einer Zahl. Die Ansichten wissen nichts von der Leiste.
import { Layers, Monitor, Package, Gauge, Users } from "lucide-vue-next";
import { WpNavList } from "@weppelmann/components";

import type { Seite } from "../api/typen";
import { ist_seite, SEITEN } from "../logik/navigation";

defineProps<{ seite: Seite }>();
const emit = defineEmits<{ waehle: [Seite] }>();

const zeichen = {
  ueberblick: Gauge,
  bibliothek: Package,
  saetze: Layers,
  gruppen: Users,
  geraete: Monitor,
};

const eintraege = SEITEN.map((eintrag) => ({
  id: eintrag.id,
  label: eintrag.titel,
  icon: zeichen[eintrag.id],
}));

function waehle(wert: string): void {
  if (ist_seite(wert)) emit("waehle", wert);
}
</script>

<template>
  <aside class="seitenleiste">
    <WpNavList
      :entries="eintraege"
      :model-value="seite"
      title="Verwaltung"
      label="Verwaltung"
      @update:model-value="waehle"
    />
  </aside>
</template>

<style scoped>
.seitenleiste {
  flex: 0 0 calc(var(--wp-space-8) * 3);
  min-height: 0;
  padding: var(--wp-space-2);
  overflow-y: auto;
  background: var(--wp-color-bg-panel);
  border-right: var(--wp-size-border) solid var(--wp-color-border-subtle);
}
</style>
