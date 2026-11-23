<script setup lang="ts">
// Eine Auswahl aus wenigen Möglichkeiten als zusammenhängende Segmentreihe
// (Farbschema, Sprache) – wie `bausteine::segmentauswahl` im Client-Fenster.
// Gewählt ist eine Akzentfläche mit Schrift in `text-on-accent`; die Knöpfe
// teilen sich die Linien zwischen sich. Höhe: `size.control-sm`.
//
// Warum nicht `WpToggleGroup`: dessen Maße stehen in Pixeln im Bauteil der
// Bibliothek und weichen von den Tokens ab. Hier liest die Reihe die Tokens,
// damit egui und Vue dasselbe Maß haben (AGENTS.md §2.11).
export interface Segment {
  wert: string;
  name: string;
}

defineProps<{
  /** Der gewählte Wert. */
  wert: string;
  moeglichkeiten: Segment[];
  /** Name der Gruppe für den Screenreader. */
  label: string;
}>();

const emit = defineEmits<{ waehle: [wert: string] }>();
</script>

<template>
  <div class="segment" role="radiogroup" :aria-label="label">
    <button
      v-for="moeglichkeit in moeglichkeiten"
      :key="moeglichkeit.wert"
      type="button"
      role="radio"
      :aria-checked="moeglichkeit.wert === wert"
      :class="['segment__knopf', { 'is-aktiv': moeglichkeit.wert === wert }]"
      @click="emit('waehle', moeglichkeit.wert)"
    >
      {{ moeglichkeit.name }}
    </button>
  </div>
</template>

<style scoped>
.segment {
  display: inline-flex;
  height: var(--wp-size-control-sm);
}

.segment__knopf {
  height: var(--wp-size-control-sm);
  padding: 0 var(--wp-space-3);
  margin-left: calc(var(--wp-size-border) * -1);
  background: var(--wp-color-bg-raised);
  border: var(--wp-size-border) solid var(--wp-color-border-strong);
  color: var(--wp-color-text-muted);
  font: inherit;
  font-size: var(--wp-font-size-sm);
  cursor: pointer;
}

.segment__knopf:first-child {
  margin-left: 0;
}

.segment__knopf:hover:not(.is-aktiv) {
  color: var(--wp-color-text-default);
}

.segment__knopf.is-aktiv {
  background: var(--wp-color-accent-default);
  border-color: var(--wp-color-accent-default);
  color: var(--wp-color-text-on-accent);
}

/* Der gewählte Knopf deckt die Linie seines linken Nachbarn zu. */
.segment__knopf.is-aktiv + .segment__knopf {
  border-left-color: var(--wp-color-accent-default);
}

.segment__knopf:focus-visible {
  outline: var(--wp-size-focus) solid var(--wp-color-focus-ring);
  outline-offset: calc(var(--wp-size-focus) * -1);
}
</style>
