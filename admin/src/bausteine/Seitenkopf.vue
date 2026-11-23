<script setup lang="ts">
// Überschrift einer Seite mit Unterzeile, wie `bausteine::seitenkopf`.
defineProps<{ titel: string; unter: string }>();
</script>

<template>
  <header class="seitenkopf">
    <h1 class="seitenkopf__titel">{{ titel }}</h1>
    <p class="seitenkopf__unter">{{ unter }}</p>
  </header>
</template>

<style scoped>
.seitenkopf {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  padding-top: var(--wp-space-4);
  padding-bottom: var(--wp-space-4);
}

.seitenkopf__titel {
  font-size: var(--wp-font-size-2xl);
  font-weight: var(--wp-font-weight-semibold);
  line-height: var(--wp-font-line-tight);
}

.seitenkopf__unter {
  color: var(--wp-color-text-muted);
}
</style>
