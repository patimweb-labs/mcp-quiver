<script setup lang="ts">
// Zustand als kleines Schild: Quadrat in der Statusfarbe, Text daneben.
// Entspricht `bausteine::status`; die Bedeutung kommt als `Ton`, nie als Farbe.
import type { Ton } from "../logik/ton";

withDefaults(defineProps<{ ton?: Ton; text?: string }>(), { ton: "normal" });
</script>

<template>
  <span class="status" :class="`status--${ton}`">
    <span
      class="status__quadrat"
      aria-hidden="true"
      :role="text ? undefined : 'img'"
      :aria-label="text ? undefined : ton"
    />
    <span v-if="text" class="status__text">{{ text }}</span>
  </span>
</template>

<style scoped>
.status {
  display: inline-flex;
  align-items: center;
  gap: var(--wp-space-1);
  min-width: 0;
}

.status__quadrat {
  flex: none;
  width: var(--wp-space-2);
  height: var(--wp-space-2);
  background: currentColor;
}

.status__text {
  overflow: hidden;
  text-overflow: ellipsis;
}

.status--normal {
  color: var(--wp-color-text-default);
}

.status--leise {
  color: var(--wp-color-text-muted);
}

.status--blass {
  color: var(--wp-color-text-faint);
}

.status--gut {
  color: var(--wp-color-status-ok);
}

.status--warnung {
  color: var(--wp-color-status-warn);
}

.status--fehler {
  color: var(--wp-color-status-error);
}

.status--akzent {
  color: var(--wp-color-accent-default);
}
</style>
