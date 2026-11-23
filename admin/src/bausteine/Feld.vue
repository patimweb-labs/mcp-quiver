<script setup lang="ts">
// Eine Formularzeile: Beschriftung, Bedienelement, Hinweis darunter. Breit
// nebeneinander, schmal untereinander – wie `bausteine::feld`. Die
// Beschriftung wird über `for`/`id` zum Namen des Elements (der Slot gibt die
// Kennung weiter).
import { useId } from "vue";

const props = defineProps<{
  beschriftung: string;
  hinweis?: string;
  fehler?: string;
  /** Eigene Kennung, wenn das Element sie schon von außen bekommt. */
  kennung?: string;
  /**
   * `false` für Bedienelemente, die keinen eigenen Namen tragen können
   * (Knopfreihen) – die Beschriftung bleibt sichtbar, verknüpft aber nichts.
   */
  benennt?: boolean;
}>();

const eigene = useId();
const id = props.kennung ?? eigene;
</script>

<template>
  <div class="feld">
    <label v-if="benennt !== false" class="feld__label" :for="id">
      {{ beschriftung }}
    </label>
    <span v-else class="feld__label">{{ beschriftung }}</span>
    <div class="feld__inhalt">
      <slot :id="id" :beschrieben="hinweis || fehler ? `${id}-hinweis` : undefined" />
      <p v-if="fehler" :id="`${id}-hinweis`" class="feld__fehler">{{ fehler }}</p>
      <p v-else-if="hinweis" :id="`${id}-hinweis`" class="feld__hinweis">
        {{ hinweis }}
      </p>
    </div>
  </div>
</template>

<style scoped>
.feld {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: var(--wp-space-1) var(--wp-space-3);
  padding-block: var(--wp-space-1);
}

.feld__label {
  flex: 0 0 calc(var(--wp-space-8) * 3);
  min-width: 0;
  padding-top: var(--wp-space-1);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.feld__inhalt {
  display: flex;
  flex: 1 1 calc(var(--wp-space-8) * 8);
  flex-direction: column;
  gap: var(--wp-space-1);
  min-width: 0;
  max-width: calc(var(--wp-space-8) * 10);
}

.feld__hinweis,
.feld__fehler {
  font-size: var(--wp-font-size-sm);
}

.feld__hinweis {
  color: var(--wp-color-text-faint);
}

.feld__fehler {
  color: var(--wp-color-status-error);
}
</style>
