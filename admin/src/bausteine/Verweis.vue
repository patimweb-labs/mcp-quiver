<script setup lang="ts">
// Ein Verweis, der aussieht wie ein Knopf: für Wege, die der Browser selbst
// gehen muss (Weiterleitung zu Entra und zurück). Kein `fetch`, kein
// `window.location` – ein echtes `<a href>`, damit Mittelklick, Kopieren und
// die Statuszeile funktionieren.
//
// Hand-Cursor und Fokusrahmen stehen hier im Baustein, nie in der Ansicht
// (AGENTS.md §2.12). Die Maße sind die von `WpButton` (secondary, md) und
// kommen aus den Tokens.
defineProps<{ href: string; label: string }>();
</script>

<template>
  <a class="verweis" :href="href">
    <span class="verweis__text">{{ label }}</span>
  </a>
</template>

<style scoped>
.verweis {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: var(--wp-size-control-md);
  padding-inline: var(--wp-size-control-padding);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  background: var(--wp-color-bg-raised);
  color: var(--wp-color-text-default);
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-medium);
  text-decoration: none;
  cursor: pointer;
}

.verweis:hover {
  background: var(--wp-color-bg-hover);
}

.verweis:active {
  background: var(--wp-color-bg-selected);
}

.verweis:focus-visible {
  outline: var(--wp-size-focus) solid var(--wp-color-focus-ring);
  outline-offset: var(--wp-size-focus);
}

.verweis__text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
