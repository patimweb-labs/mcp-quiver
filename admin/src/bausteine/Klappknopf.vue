<script setup lang="ts">
// Ein Knopf, der eine Zeile auf- und zuklappt: Pfeil, Name, Hand-Cursor. Der
// Hand-Cursor gehört hierher in den Baustein, nicht in die Ansicht
// (AGENTS.md §2.12).
import { ChevronRight } from "lucide-vue-next";

defineProps<{ offen: boolean; steuert: string }>();
const emit = defineEmits<{ umschalten: [] }>();
</script>

<template>
  <button
    type="button"
    class="klappknopf"
    :aria-expanded="offen"
    :aria-controls="steuert"
    @click="emit('umschalten')"
  >
    <ChevronRight
      class="klappknopf__pfeil"
      :class="{ 'ist-offen': offen }"
      aria-hidden="true"
    />
    <span class="klappknopf__text"><slot /></span>
  </button>
</template>

<style scoped>
.klappknopf {
  display: inline-flex;
  align-items: center;
  gap: var(--wp-space-1);
  padding: 0;
  background: none;
  border: 0;
  color: var(--wp-color-text-default);
  font: inherit;
  font-weight: var(--wp-font-weight-medium);
  cursor: pointer;
  text-align: start;
}

.klappknopf:focus-visible {
  outline: var(--wp-size-focus) solid var(--wp-color-focus-ring);
  outline-offset: var(--wp-size-focus);
}

.klappknopf__pfeil {
  flex: none;
  width: var(--wp-font-size-md);
  height: var(--wp-font-size-md);
  color: var(--wp-color-text-muted);
  transition: transform var(--wp-duration-fast);
}

.klappknopf__pfeil.ist-offen {
  transform: rotate(90deg);
}

.klappknopf__text {
  min-width: 0;
  overflow-wrap: anywhere;
}
</style>
