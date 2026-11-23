<script setup lang="ts">
// Ein Kasten wie eine Karte, ohne Titel (Listen, leere Ansichten).
// Entspricht `bausteine::kasten`.
</script>

<template>
  <div class="kasten">
    <slot />
  </div>
</template>

<style scoped>
.kasten {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
  min-width: 0;
  padding: var(--wp-space-4);
  background: var(--wp-color-bg-panel);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
}
</style>
