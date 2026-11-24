<script setup lang="ts">
// Eine Karte: Rahmen, Titel, Unterzeile, Inhalt. Gleiche Maße wie
// `bausteine::karte` im Client-Fenster.
//
// `titel_als` ist die Ebene der Überschrift: Auf einer Seite, die nur aus
// dieser Karte besteht (Anmelden, geschlossen), ist der Titel die h1; sonst
// bleibt er eine h2 unter dem Seitenkopf. So hat jede Seite genau eine h1.
withDefaults(
  defineProps<{ titel: string; unter?: string; titel_als?: "h1" | "h2" }>(),
  { titel_als: "h2" },
);
</script>

<template>
  <section class="karte">
    <header class="karte__kopf">
      <div class="karte__zeile">
        <component :is="titel_als" class="karte__titel">{{ titel }}</component>
        <div v-if="$slots.aktionen" class="karte__aktionen">
          <slot name="aktionen" />
        </div>
      </div>
      <p v-if="unter" class="karte__unter">{{ unter }}</p>
    </header>
    <div class="karte__inhalt">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.karte {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
  min-width: 0;
  padding: var(--wp-space-4);
  background: var(--wp-color-bg-panel);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.karte__kopf {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
}

.karte__zeile {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--wp-space-3);
  flex-wrap: wrap;
}

.karte__titel {
  font-size: var(--wp-font-size-lg);
  font-weight: var(--wp-font-weight-semibold);
  line-height: var(--wp-font-line-tight);
}

.karte__unter {
  color: var(--wp-color-text-muted);
}

.karte__aktionen {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.karte__inhalt {
  min-width: 0;
}
</style>
