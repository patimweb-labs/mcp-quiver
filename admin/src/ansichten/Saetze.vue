<script setup lang="ts">
// Sätze: benannte Sammlungen von Einträgen mit vorbelegten Server-Werten.
import { onMounted, ref, shallowRef } from "vue";
import { Layers, Pencil, Plus, RefreshCw, Trash2 } from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpIconButton,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { fehlertext, type Client } from "../api/client";
import type { Eintrag, Satz } from "../api/typen";
import Karte from "../bausteine/Karte.vue";
import Kasten from "../bausteine/Kasten.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import { neuer_satz, saetze_sortiert } from "../logik/satz";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";
import SatzDialog from "./SatzDialog.vue";

const props = defineProps<{ client: Client }>();

interface Daten {
  saetze: Satz[];
  eintraege: Eintrag[];
}

const zustand = shallowRef<Zustand<Daten>>(laedt());
const meldung = ref("");
const dialogOffen = ref(false);
const neuModus = ref(false);
const bearbeitet = shallowRef<Satz | null>(null);

const zuLoeschen = shallowRef<Satz | null>(null);
const loeschfehler = ref("");
const loescht = ref(false);

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    const [saetze, eintraege] = await Promise.all([
      props.client.saetze(),
      props.client.eintraege(),
    ]);
    zustand.value = fertig({ saetze: saetze_sortiert(saetze), eintraege });
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

function neu(): void {
  meldung.value = "";
  neuModus.value = true;
  bearbeitet.value = neuer_satz();
  dialogOffen.value = true;
}

function bearbeiten(satz: Satz): void {
  meldung.value = "";
  neuModus.value = false;
  bearbeitet.value = tief_kopie(satz);
  dialogOffen.value = true;
}

async function gespeichert(satz: Satz): Promise<void> {
  meldung.value = `Satz ${satz.id} gespeichert.`;
  await laden();
}

async function entfernen(): Promise<void> {
  const ziel = zuLoeschen.value;
  if (!ziel) return;
  loescht.value = true;
  loeschfehler.value = "";
  try {
    await props.client.satzLoeschen(ziel.id);
    zuLoeschen.value = null;
    meldung.value = `Satz ${ziel.id} entfernt.`;
    await laden();
  } catch (fehlerWert) {
    loeschfehler.value = fehlertext(fehlerWert);
  } finally {
    loescht.value = false;
  }
}

function schliesseLoeschen(): void {
  zuLoeschen.value = null;
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Sätze"
      unter="Sammlungen von Einträgen; Gruppen bekommen sie zugewiesen."
    />

    <div class="werkzeuge">
      <WpButton label="Neu" variant="primary" :icon="Plus" @press="neu" />
      <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
    </div>

    <p v-if="meldung" class="meldung" role="status">
      <Status ton="gut" :text="meldung" />
    </p>

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Sätze und Einträge werden geholt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </Karte>
    </template>

    <template v-else-if="zustand.daten.saetze.length === 0">
      <Leer
        titel="Noch keine Sätze"
        erklaerung="Ein Satz bündelt Einträge, die zusammen auf einem Gerät laufen sollen."
      >
        <template #aktion>
          <WpButton label="Neu" variant="primary" :icon="Plus" @press="neu" />
        </template>
      </Leer>
    </template>

    <Kasten v-else>
      <div class="liste">
        <article
          v-for="satz in zustand.daten.saetze"
          :key="satz.id"
          class="zeile"
        >
          <Layers class="zeile__zeichen" aria-hidden="true" />
          <div class="zeile__haupt">
            <div class="zeile__kopf">
              <h3 class="zeile__titel">{{ satz.titel }}</h3>
              <span class="marke-chip">Priorität {{ satz.prioritaet }}</span>
              <span class="marke-chip">
                {{ satz.eintraege.length }}
                {{ satz.eintraege.length === 1 ? "Eintrag" : "Einträge" }}
              </span>
            </div>
            <p class="mono blass">{{ satz.id }}</p>
          </div>
          <div class="zeile__aktionen">
            <WpIconButton
              :icon="Pencil"
              label="Bearbeiten"
              @press="bearbeiten(satz)"
            />
            <WpIconButton
              :icon="Trash2"
              label="Entfernen"
              variant="ghost"
              @press="zuLoeschen = satz"
            />
          </div>
        </article>
      </div>
    </Kasten>

    <SatzDialog
      v-model:offen="dialogOffen"
      :satz="bearbeitet"
      :neu="neuModus"
      :eintraege="zustand.art === 'fertig' ? zustand.daten.eintraege : []"
      :client="client"
      @gespeichert="gespeichert"
    />

    <WpDialog
      :open="zuLoeschen !== null"
      title="Satz entfernen"
      :description="
        zuLoeschen
          ? `${zuLoeschen.titel} (${zuLoeschen.id}) wird entfernt. Gruppen, die ihn zuweisen, verhindern das.`
          : ''
      "
      tone="error"
      @update:open="(wert) => (!wert ? schliesseLoeschen() : undefined)"
    >
      <p v-if="loeschfehler" class="fehler" role="alert">{{ loeschfehler }}</p>
      <template #actions>
        <WpButton label="Abbrechen" @press="schliesseLoeschen" />
        <WpButton
          label="Entfernen"
          variant="danger"
          :state="loescht ? 'busy' : 'default'"
          @press="entfernen"
        />
      </template>
    </WpDialog>
  </div>
</template>

<style scoped>
.seite {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-4);
}

.werkzeuge {
  display: flex;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.meldung {
  display: flex;
}

.liste {
  display: flex;
  flex-direction: column;
}

.zeile {
  display: flex;
  align-items: flex-start;
  gap: var(--wp-space-3);
  padding-block: var(--wp-space-3);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.zeile:first-child {
  padding-top: 0;
}

.zeile:last-child {
  padding-bottom: 0;
  border-bottom: 0;
}

.zeile__zeichen {
  flex: none;
  width: var(--wp-font-size-lg);
  height: var(--wp-font-size-lg);
  margin-top: var(--wp-space-1);
  color: var(--wp-color-text-muted);
}

.zeile__haupt {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--wp-space-1);
  min-width: 0;
}

.zeile__kopf {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.zeile__titel {
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-semibold);
}

.zeile__aktionen {
  display: flex;
  gap: var(--wp-space-1);
  flex: none;
}

.marke-chip {
  padding: 0 var(--wp-space-2);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  background: var(--wp-color-bg-raised);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-xs);
  line-height: var(--wp-size-control-sm);
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
