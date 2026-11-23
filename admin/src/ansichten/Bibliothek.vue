<script setup lang="ts">
// Die Bibliothek: alle Einträge, Suche, Anlegen, Bearbeiten, Importieren,
// Entfernen. Die Liste ist das Modell; gelöscht wird erst nach Bestätigung.
import { computed, onMounted, ref, shallowRef } from "vue";
import {
  Package,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  Upload,
} from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpIconButton,
  WpInput,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { fehlertext, type Client } from "../api/client";
import type { Eintrag } from "../api/typen";
import Karte from "../bausteine/Karte.vue";
import Kasten from "../bausteine/Kasten.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import {
  filter_eintraege,
  herkunft_text,
  laufzeit_text,
  neuer_eintrag,
  sortiert_nach_titel,
} from "../logik/eintrag";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";
import EintragDialog from "./EintragDialog.vue";
import ImportDialog from "./ImportDialog.vue";

const props = defineProps<{ client: Client }>();

const zustand = shallowRef<Zustand<Eintrag[]>>(laedt());
const suche = ref("");
const meldung = ref("");

const dialogOffen = ref(false);
const neuModus = ref(false);
const bearbeitet = shallowRef<Eintrag | null>(null);
const importOffen = ref(false);

const zuLoeschen = shallowRef<Eintrag | null>(null);
const loeschfehler = ref("");
const loescht = ref(false);

const gefiltert = computed(() => {
  if (zustand.value.art !== "fertig") return [];
  return filter_eintraege(sortiert_nach_titel(zustand.value.daten), suche.value);
});

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    zustand.value = fertig(await props.client.eintraege());
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

function neu(): void {
  meldung.value = "";
  neuModus.value = true;
  bearbeitet.value = neuer_eintrag();
  dialogOffen.value = true;
}

function bearbeiten(eintrag: Eintrag): void {
  meldung.value = "";
  neuModus.value = false;
  bearbeitet.value = tief_kopie(eintrag);
  dialogOffen.value = true;
}

async function gespeichert(eintrag: Eintrag): Promise<void> {
  meldung.value = `Eintrag ${eintrag.id} gespeichert.`;
  await laden();
}

async function importiert(eintrag: Eintrag): Promise<void> {
  meldung.value = `Eintrag ${eintrag.id} importiert.`;
  await laden();
}

async function entfernen(): Promise<void> {
  const ziel = zuLoeschen.value;
  if (!ziel) return;
  loescht.value = true;
  loeschfehler.value = "";
  try {
    await props.client.eintragLoeschen(ziel.id);
    zuLoeschen.value = null;
    meldung.value = `Eintrag ${ziel.id} entfernt.`;
    await laden();
  } catch (fehlerWert) {
    loeschfehler.value = fehlertext(fehlerWert);
  } finally {
    loescht.value = false;
  }
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Bibliothek"
      unter="Jedes MCP-Tool, das der Server verteilen kann."
    />

    <div class="werkzeuge">
      <div class="werkzeuge__suche">
        <WpInput
          v-model="suche"
          type="search"
          placeholder="Suchen (Kennung, Titel, Beschreibung)"
          aria-label="Einträge durchsuchen"
        />
      </div>
      <div class="werkzeuge__knoepfe">
        <WpButton label="Neu" variant="primary" :icon="Plus" @press="neu" />
        <WpButton
          label="Importieren"
          :icon="Upload"
          @press="importOffen = true"
        />
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </div>
    </div>

    <p v-if="meldung" class="meldung" role="status">
      <Status ton="gut" :text="meldung" />
    </p>

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Die Einträge werden geholt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </Karte>
    </template>

    <template v-else-if="gefiltert.length === 0">
      <Leer
        v-if="zustand.daten.length === 0"
        titel="Noch keine Einträge"
        erklaerung="Lege den ersten Eintrag an oder importiere eine server.json."
      >
        <template #aktion>
          <WpButton label="Neu" variant="primary" :icon="Plus" @press="neu" />
        </template>
      </Leer>
      <Leer
        v-else
        titel="Keine Treffer"
        :erklaerung="`Nichts passt zu „${suche}“.`"
      >
        <template #aktion>
          <WpButton label="Suche leeren" @press="suche = ''" />
        </template>
      </Leer>
    </template>

    <Kasten v-else>
      <div class="eintraege">
        <article
          v-for="eintrag in gefiltert"
          :key="eintrag.id"
          class="eintrag"
        >
          <Package class="eintrag__zeichen" aria-hidden="true" />
          <div class="eintrag__haupt">
            <div class="eintrag__kopf">
              <h3 class="eintrag__titel">{{ eintrag.titel }}</h3>
              <span class="marke-chip">{{ eintrag.version }}</span>
              <span class="marke-chip">{{ herkunft_text(eintrag.herkunft) }}</span>
              <span class="marke-chip">{{ laufzeit_text(eintrag.laufzeit) }}</span>
            </div>
            <p v-if="eintrag.beschreibung" class="eintrag__beschreibung">
              {{ eintrag.beschreibung }}
            </p>
            <p class="mono blass">{{ eintrag.id }}</p>
          </div>
          <div class="eintrag__aktionen">
            <WpIconButton
              :icon="Pencil"
              label="Bearbeiten"
              @press="bearbeiten(eintrag)"
            />
            <WpIconButton
              :icon="Trash2"
              label="Entfernen"
              variant="ghost"
              @press="zuLoeschen = eintrag"
            />
          </div>
        </article>
      </div>
    </Kasten>

    <EintragDialog
      v-model:offen="dialogOffen"
      :eintrag="bearbeitet"
      :neu="neuModus"
      :client="client"
      @gespeichert="gespeichert"
    />

    <ImportDialog
      v-model:offen="importOffen"
      :client="client"
      @importiert="importiert"
    />

    <WpDialog
      :open="zuLoeschen !== null"
      title="Eintrag entfernen"
      :description="
        zuLoeschen
          ? `${zuLoeschen.titel} (${zuLoeschen.id}) wird aus der Bibliothek genommen. Sätze, die ihn benutzen, verhindern das.`
          : ''
      "
      tone="error"
      @update:open="(wert) => (!wert ? (zuLoeschen = null) : undefined)"
    >
      <p v-if="loeschfehler" class="fehler" role="alert">{{ loeschfehler }}</p>
      <template #actions>
        <WpButton label="Abbrechen" @press="zuLoeschen = null" />
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
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-3);
  flex-wrap: wrap;
}

.werkzeuge__suche {
  flex: 1 1 calc(var(--wp-space-8) * 5);
  max-width: calc(var(--wp-space-8) * 8);
  min-width: 0;
}

.werkzeuge__knoepfe {
  display: flex;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.meldung {
  display: flex;
}

.eintraege {
  display: flex;
  flex-direction: column;
}

.eintrag {
  display: flex;
  align-items: flex-start;
  gap: var(--wp-space-3);
  padding-block: var(--wp-space-3);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.eintrag:last-child {
  padding-bottom: 0;
  border-bottom: 0;
}

.eintrag:first-child {
  padding-top: 0;
}

.eintrag__zeichen {
  flex: none;
  width: var(--wp-font-size-lg);
  height: var(--wp-font-size-lg);
  margin-top: var(--wp-space-1);
  color: var(--wp-color-text-muted);
}

.eintrag__haupt {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--wp-space-1);
  min-width: 0;
}

.eintrag__kopf {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.eintrag__titel {
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-semibold);
}

.eintrag__beschreibung {
  color: var(--wp-color-text-muted);
}

.eintrag__aktionen {
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
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
