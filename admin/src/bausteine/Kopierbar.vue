<script setup lang="ts">
// Ein Wert zum Weitergeben (Code, Kennung): umbrechend, daneben ein Knopf zum
// Kopieren. Entspricht `bausteine::kopierbar`.
import { onBeforeUnmount, ref } from "vue";
import { Check, Copy } from "lucide-vue-next";
import { WpIconButton } from "@weppelmann/components";

const props = withDefaults(
  defineProps<{ wert: string; kopieren?: string }>(),
  { kopieren: "Kopieren" },
);

const kopiert = ref(false);
let uhr: ReturnType<typeof setTimeout> | undefined;

async function inZwischenablage(): Promise<void> {
  try {
    await navigator.clipboard.writeText(props.wert);
    kopiert.value = true;
    clearTimeout(uhr);
    uhr = setTimeout(() => {
      kopiert.value = false;
    }, 1500);
  } catch {
    // Ohne Zwischenablage bleibt der Wert sichtbar und markierbar.
  }
}

onBeforeUnmount(() => clearTimeout(uhr));
</script>

<template>
  <span class="kopierbar">
    <WpIconButton
      :icon="kopiert ? Check : Copy"
      :label="kopiert ? 'Kopiert' : kopieren"
      size="sm"
      variant="ghost"
      @press="inZwischenablage"
    />
    <span class="kopierbar__wert mono">{{ wert }}</span>
  </span>
</template>

<style scoped>
.kopierbar {
  display: inline-flex;
  align-items: center;
  gap: var(--wp-space-1);
  min-width: 0;
}

.kopierbar__wert {
  min-width: 0;
  overflow-wrap: anywhere;
}
</style>
