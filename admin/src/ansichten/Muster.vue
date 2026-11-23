<script setup lang="ts">
// Die Musterseite: dieselbe Seite wie `crates/oberflaeche/tests/muster.rs` im
// Client-Fenster — Kopf, Seitenleiste mit drei Einträgen, Seitenkopf, eine
// Karte mit Hauptknopf und Knopf, drei Statuszeilen und ein Eingabefeld.
//
// Sie steht nur im Entwicklungsbau unter `/muster` und braucht keine
// Anmeldung: sie zeigt keine Daten, nur die Bausteine (AGENTS.md §2.11).
import { ref, watch } from "vue";
import { Plus, RefreshCw } from "lucide-vue-next";
import { WpButton, WpInput } from "@weppelmann/components";

import type { Seite, Thema } from "../api/typen";
import { thema_aus_wert, THEMA_SPEICHER } from "../logik/thema";
import Feld from "../bausteine/Feld.vue";
import Karte from "../bausteine/Karte.vue";
import Kopf from "../bausteine/Kopf.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Seitenleiste from "../bausteine/Seitenleiste.vue";
import Status from "../bausteine/Status.vue";

const thema = ref<Thema>(thema_aus_wert(localStorage.getItem(THEMA_SPEICHER)));
const seite = ref<Seite>("bibliothek");
const suche = ref("");

const DREI: Seite[] = ["bibliothek", "saetze", "geraete"];

watch(
  thema,
  (wert) => {
    document.documentElement.dataset.theme = wert;
    localStorage.setItem(THEMA_SPEICHER, wert);
  },
  { immediate: true },
);

function setzeThema(wert: string): void {
  thema.value = thema_aus_wert(wert);
}
</script>

<template>
  <div class="muster">
    <Kopf :thema="thema" @thema="setzeThema" @abmelden="() => {}" />
    <div class="muster__koerper">
      <Seitenleiste :seite="seite" :ids="DREI" @waehle="seite = $event" />
      <main class="muster__inhalt">
        <div class="stapel">
          <Seitenkopf
            titel="Bibliothek"
            unter="Jedes MCP-Tool, das der Server verteilen kann."
          />
          <Karte titel="Einträge" unter="Drei Zustände, wie sie die Seite zeigt.">
            <div class="stapel stapel--dicht">
              <div class="reihe">
                <WpButton
                  label="Neu"
                  variant="primary"
                  :icon="Plus"
                  @press="() => {}"
                />
                <WpButton label="Neu laden" :icon="RefreshCw" @press="() => {}" />
              </div>
              <div class="statusliste">
                <Status ton="gut" text="Läuft · 2 Tools" />
                <Status ton="warnung" text="Braucht Angaben: PFAD" />
                <Status ton="fehler" text="Fehler" />
              </div>
              <Feld beschriftung="Suche" hinweis="Kennung, Titel, Beschreibung">
                <template #default="{ id }">
                  <WpInput :id="id" v-model="suche" placeholder="Suchen" />
                </template>
              </Feld>
            </div>
          </Karte>
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
.muster {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--wp-color-bg-canvas);
}

.muster__koerper {
  display: flex;
  flex: 1;
  min-height: 0;
}

.muster__inhalt {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding-inline: var(--wp-space-5);
  padding-bottom: var(--wp-space-6);
}

/* Abstände wie im Client-Fenster: 16 px zwischen den Blöcken, 4 px zwischen
   den Statuszeilen. */
.stapel {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-4);
}

/* Im Karteninhalt: 12 px zwischen den Blöcken. */
.stapel--dicht {
  gap: var(--wp-space-3);
}

.reihe {
  display: flex;
  gap: var(--wp-space-2);
}

.statusliste {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
}
</style>
