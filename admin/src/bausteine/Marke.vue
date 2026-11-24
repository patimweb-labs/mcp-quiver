<script setup lang="ts">
// Die Marke: Zeichen und Wortmarke – wie `bausteine::marke` im Client-Fenster.
// Das Zeichen folgt dem Farbschema und ist Schmuck (leeres `alt`), den Namen
// trägt die Wortmarke.
import { computed } from "vue";

import type { Thema } from "../api/typen";
import dunkel from "../marke/logo-dunkel.png";
import hell from "../marke/logo-hell.png";
import mono from "../marke/logo-mono.png";

const props = defineProps<{ thema: Thema }>();

const ZEICHEN: Record<Thema, string> = { dark: dunkel, light: hell, mono };
const zeichen = computed(() => ZEICHEN[props.thema]);
</script>

<template>
  <span class="marke-gruppe">
    <img class="marke-gruppe__zeichen" :src="zeichen" alt="" />
    <span class="marke">mcp-quiver</span>
  </span>
</template>

<style scoped>
.marke-gruppe {
  display: inline-flex;
  align-items: center;
  gap: var(--wp-space-2);
}

.marke-gruppe__zeichen {
  width: var(--wp-size-logo);
  height: var(--wp-size-logo);
}
</style>
