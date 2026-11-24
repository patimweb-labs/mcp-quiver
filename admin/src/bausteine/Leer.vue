<script setup lang="ts">
// Eine leere Fläche mit Titel und Erklärung, wie `bausteine::leer`.
import Kasten from "./Kasten.vue";

defineProps<{ titel: string; erklaerung?: string }>();
</script>

<template>
  <Kasten>
    <p class="leer__titel">{{ titel }}</p>
    <p v-if="erklaerung" class="leer__text">{{ erklaerung }}</p>
    <div v-if="$slots.aktion" class="leer__aktion">
      <slot name="aktion" />
    </div>
  </Kasten>
</template>

<style scoped>
.leer__titel {
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-semibold);
}

.leer__text {
  color: var(--wp-color-text-muted);
}

.leer__aktion {
  display: flex;
  gap: var(--wp-space-2);
  padding-top: var(--wp-space-2);
}
</style>
