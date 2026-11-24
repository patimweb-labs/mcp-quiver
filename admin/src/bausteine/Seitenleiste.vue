<script setup lang="ts">
// Navigation links: Abschnittsname, darunter eine Zeile je Seite mit Zeichen
// und Name. Dasselbe Muster wie `bausteine::seitenleiste` im Client-Fenster:
// Zeilenhöhe `size.navzeile`, gewählt = ruhige Fläche + Akzentbalken links.
//
// Warum nicht `WpNavList`: dessen Zeilenmaße stehen in Pixeln im Bauteil der
// Bibliothek. Hier liest jede Zeile die Tokens (AGENTS.md §2.11).
import { computed } from "vue";
import { Gauge, Layers, Monitor, Package, ScrollText, UserRound, Users } from "lucide-vue-next";

import type { Seite } from "../api/typen";
import { SEITEN } from "../logik/navigation";

const props = defineProps<{
  seite: Seite;
  /** Nur diese Seiten zeigen (die Musterseite zeigt drei). */
  ids?: Seite[];
}>();

const emit = defineEmits<{ waehle: [Seite] }>();

const zeichen = {
  ueberblick: Gauge,
  bibliothek: Package,
  saetze: Layers,
  gruppen: Users,
  benutzer: UserRound,
  geraete: Monitor,
  pruefspur: ScrollText,
};

const eintraege = computed(() =>
  SEITEN.filter((eintrag) => !props.ids || props.ids.includes(eintrag.id)).map(
    (eintrag) => ({
      id: eintrag.id,
      name: eintrag.titel,
      icon: zeichen[eintrag.id],
    }),
  ),
);

function waehle(wert: Seite): void {
  emit("waehle", wert);
}
</script>

<template>
  <aside class="seitenleiste">
    <nav class="nav" aria-label="Verwaltung">
      <p class="nav__titel">Verwaltung</p>
      <button
        v-for="eintrag in eintraege"
        :key="eintrag.id"
        type="button"
        :class="['nav__eintrag', { 'is-aktiv': eintrag.id === seite }]"
        :aria-current="eintrag.id === seite ? 'page' : undefined"
        :aria-label="eintrag.name"
        :title="eintrag.name"
        @click="waehle(eintrag.id)"
      >
        <component :is="eintrag.icon" class="nav__zeichen" aria-hidden="true" />
        <span class="nav__name">{{ eintrag.name }}</span>
      </button>
    </nav>
  </aside>
</template>

<style scoped>
.seitenleiste {
  flex: 0 0 var(--wp-size-seitenleiste);
  min-height: 0;
  padding: var(--wp-space-2);
  overflow-y: auto;
  background: var(--wp-color-bg-panel);
  border-right: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

/* Unter `breakpoint.schmal` trägt die Leiste nur die Zeichen; der Name steht
   im Hinweis und als zugänglicher Name (Aufgabe 18, wie im Client-Fenster). */
:root[data-breite="eng"] .seitenleiste,
:root[data-breite="schmal"] .seitenleiste {
  flex-basis: var(--wp-size-seitenleiste-schmal);
}

:root[data-breite="eng"] .nav__titel,
:root[data-breite="eng"] .nav__name,
:root[data-breite="schmal"] .nav__titel,
:root[data-breite="schmal"] .nav__name {
  display: none;
}

:root[data-breite="eng"] .nav__eintrag,
:root[data-breite="schmal"] .nav__eintrag {
  justify-content: center;
  padding-inline: var(--wp-space-2);
  border-left-width: 0;
}

:root[data-breite="eng"] .nav__eintrag.is-aktiv,
:root[data-breite="schmal"] .nav__eintrag.is-aktiv {
  box-shadow: inset var(--wp-size-akzentbalken) 0 0
    var(--wp-color-accent-default);
}

.nav {
  display: flex;
  flex-direction: column;
}

/* Der Abschnittsname ist eine Zeile hoch wie ein Eintrag. */
.nav__titel {
  display: flex;
  align-items: center;
  height: var(--wp-size-navzeile);
  padding-inline: var(--wp-space-2);
  color: var(--wp-color-text-faint);
  font-size: var(--wp-font-size-xs);
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.nav__eintrag {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  width: 100%;
  height: var(--wp-size-navzeile);
  /* Der Balken zählt zum Kasten; um seine Breite rückt der Inhalt nach. */
  padding-inline: calc(var(--wp-space-2) - var(--wp-size-akzentbalken))
    var(--wp-space-2);
  background: none;
  border: 0;
  border-left: var(--wp-size-akzentbalken) solid transparent;
  color: var(--wp-color-text-muted);
  font: inherit;
  font-size: var(--wp-font-size-md);
  text-align: left;
  cursor: pointer;
}

.nav__eintrag:hover {
  background: var(--wp-color-bg-hover);
  color: var(--wp-color-text-default);
}

.nav__eintrag.is-aktiv {
  background: var(--wp-color-bg-raised);
  border-left-color: var(--wp-color-accent-default);
  color: var(--wp-color-text-default);
}

.nav__eintrag.is-aktiv .nav__zeichen {
  color: var(--wp-color-accent-default);
}

.nav__eintrag:focus-visible {
  outline: var(--wp-size-focus) solid var(--wp-color-focus-ring);
  outline-offset: calc(var(--wp-size-focus) * -1);
}

.nav__zeichen {
  flex: none;
  width: var(--wp-font-size-md);
  height: var(--wp-font-size-md);
}

.nav__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
