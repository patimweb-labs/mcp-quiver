<script setup lang="ts">
// Die Verwaltung: Anmeldung, Kopf, Navigation links, Seite rechts.
//
// Alles, was die Seite ausmacht, steht im Modell (Token, Theme, Seite). Die
// Ansichten bekommen den Client und zeichnen; ihre eigenen Zustände sind
// Summentypen (`Zustand`).
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import type { Seite, Thema } from "./api/typen";
import { aufNichtAngemeldet, Client } from "./api/client";
import {
  hash_aus_seite,
  seite_aus_hash,
  seite_titel,
} from "./logik/navigation";
import {
  thema_aus_wert,
  THEMA_SPEICHER,
} from "./logik/thema";

import Anmeldung from "./ansichten/Anmeldung.vue";
import Bibliothek from "./ansichten/Bibliothek.vue";
import Geraete from "./ansichten/Geraete.vue";
import Gruppen from "./ansichten/Gruppen.vue";
import Saetze from "./ansichten/Saetze.vue";
import Ueberblick from "./ansichten/Ueberblick.vue";
import Kopf from "./bausteine/Kopf.vue";
import Seitenleiste from "./bausteine/Seitenleiste.vue";

const TOKEN_SPEICHER = "quiver-admin-token";

const token = ref<string>(sessionStorage.getItem(TOKEN_SPEICHER) ?? "");
const thema = ref<Thema>(
  thema_aus_wert(localStorage.getItem(THEMA_SPEICHER)),
);
const seite = ref<Seite>(seite_aus_hash(window.location.hash));
const anmeldefehler = ref("");

const client = computed(() => (token.value.length > 0 ? new Client(token.value) : null));

const ANSICHTEN = {
  ueberblick: Ueberblick,
  bibliothek: Bibliothek,
  saetze: Saetze,
  gruppen: Gruppen,
  geraete: Geraete,
} as const;

const ansicht = computed(() => ANSICHTEN[seite.value]);

function setzeThema(wert: string): void {
  thema.value = thema_aus_wert(wert);
}

watch(
  thema,
  (wert) => {
    document.documentElement.dataset.theme = wert;
    localStorage.setItem(THEMA_SPEICHER, wert);
  },
  { immediate: true },
);

watch(
  seite,
  (wert) => {
    document.title = `${seite_titel(wert)} · mcp-quiver`;
    const ziel = hash_aus_seite(wert);
    if (window.location.hash !== ziel) {
      window.history.replaceState(null, "", ziel);
    }
  },
  { immediate: true },
);

function beimHash(): void {
  seite.value = seite_aus_hash(window.location.hash);
}

function waehleSeite(wert: Seite): void {
  seite.value = wert;
}

function angemeldet(wert: string): void {
  token.value = wert;
  sessionStorage.setItem(TOKEN_SPEICHER, wert);
  anmeldefehler.value = "";
}

function abmelden(): void {
  token.value = "";
  sessionStorage.removeItem(TOKEN_SPEICHER);
}

onMounted(() => {
  window.addEventListener("hashchange", beimHash);
  // Ein 401 heißt: das Token gilt nicht mehr. Also zurück zur Anmeldung und
  // sagen, warum.
  aufNichtAngemeldet(() => {
    abmelden();
    anmeldefehler.value = "Die Anmeldung ist abgelaufen. Bitte erneut anmelden.";
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("hashchange", beimHash);
  aufNichtAngemeldet(null);
});
</script>

<template>
  <div class="wp-root app">
    <template v-if="client">
      <Kopf :thema="thema" @thema="setzeThema" @abmelden="abmelden" />
      <div class="app__koerper">
        <Seitenleiste :seite="seite" @waehle="waehleSeite" />
        <main class="app__inhalt">
          <component :is="ansicht" :client="client" />
        </main>
      </div>
    </template>
    <Anmeldung
      v-else
      :fehler="anmeldefehler"
      @angemeldet="angemeldet"
    />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--wp-color-bg-canvas);
}

.app__koerper {
  display: flex;
  flex: 1;
  min-height: 0;
}

.app__inhalt {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  /* Höchstens `size.inhalt-max` breit und mittig; der Rand ist mindestens
     `space-5` (Aufgabe 18). */
  padding-inline: max(
    var(--wp-space-5),
    calc((100% - var(--wp-size-inhalt-max)) / 2)
  );
  padding-bottom: var(--wp-space-6);
}
</style>
