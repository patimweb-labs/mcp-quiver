<script setup lang="ts">
// Prüfspur (Aufgabe 21): was wann wer getan hat – nur lesend.
//
// Geladen wird in Schritten: Der Server bekommt eine Zeile mehr, als gezeigt
// wird. Kommt sie zurück, gibt es weitere Einträge, und „Nachladen“ lügt
// nicht. Filter und Suche sind reine Funktionen in `logik/pruefspur.ts`.
import { computed, onMounted, ref, shallowRef } from "vue";
import { RefreshCw, ScrollText } from "lucide-vue-next";
import {
  WpButton,
  WpInput,
  WpSelect,
} from "@weppelmann/components";
import type { ListOption } from "@weppelmann/components";

import { fehlertext, type Client } from "../api/client";
import type { PruefspurEintrag } from "../api/typen";
import Karte from "../bausteine/Karte.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import { datum, zeitpunkt } from "../logik/format";
import {
  type Artfilter,
  type Pruefspurfilter,
  PRUEFSPUR_ERSTE,
  anfrage_limit,
  art_titel,
  art_ton,
  arten_im_bestand,
  eintraege_text,
  filtern,
  hat_mehr,
  naechstes_limit,
  sichtbar,
} from "../logik/pruefspur";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";

const props = defineProps<{ client: Client }>();

interface Daten {
  /** Die geladenen Zeilen (eine mehr als sichtbar, als Probe). */
  geladen: PruefspurEintrag[];
  /** Wie viele Zeilen die Ansicht zeigt. */
  limit: number;
  mehr: boolean;
}

const zustand = shallowRef<Zustand<Daten>>(laedt());
const suche = ref("");
const artfilter = ref<Artfilter>("alle");
const nachladen = ref(false);

const filter = computed<Pruefspurfilter>(() => ({
  suche: suche.value,
  art: artfilter.value,
}));

const daten = computed(() =>
  zustand.value.art === "fertig" ? zustand.value.daten : null,
);

const sichtbareZeilen = computed(() =>
  daten.value ? sichtbar(daten.value.geladen, daten.value.limit) : [],
);

const gefiltert = computed(() =>
  filtern(sichtbareZeilen.value, filter.value),
);

const artOptionen = computed<ListOption[]>(() => [
  { value: "alle", label: "Alle Arten" },
  ...arten_im_bestand(sichtbareZeilen.value).map((eintrag) => ({
    value: eintrag.wert,
    label: eintrag.titel,
  })),
]);

function setzeArt(wert: string | null): void {
  artfilter.value = wert && wert !== "alle" ? wert : "alle";
}

/** Laden mit einem Limit; das Ergebnis entscheidet, ob es mehr gibt. */
async function hole(limit: number, mitLadeanzeige: boolean): Promise<void> {
  if (mitLadeanzeige) zustand.value = laedt();
  const geladen = await props.client.pruefspur(anfrage_limit(limit));
  zustand.value = fertig({
    geladen,
    limit,
    mehr: hat_mehr(geladen.length, limit),
  });
}

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    await hole(PRUEFSPUR_ERSTE, false);
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

async function weiter(): Promise<void> {
  const alt = daten.value;
  if (!alt) return;
  nachladen.value = true;
  try {
    await hole(naechstes_limit(alt.limit), false);
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  } finally {
    nachladen.value = false;
  }
}

function filterLeeren(): void {
  suche.value = "";
  artfilter.value = "alle";
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Prüfspur"
      unter="Anmeldungen, Fehlschläge, Rollen und Änderungen – nur lesend."
    />

    <div class="werkzeuge">
      <div class="werkzeuge__suche">
        <WpInput
          v-model="suche"
          type="search"
          placeholder="Suchen (wer, was)"
          aria-label="Prüfspur durchsuchen"
        />
      </div>
      <div class="werkzeuge__filter">
        <WpSelect
          :model-value="artfilter"
          :options="artOptionen"
          label="Art"
          @update:model-value="setzeArt"
        />
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </div>
    </div>

    <p class="hinweis">
      <ScrollText class="hinweis__zeichen" aria-hidden="true" />
      Einträge lassen sich hier nicht ändern oder löschen; sie bleiben 180 Tage.
    </p>

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Die Prüfspur wird geholt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </Karte>
    </template>

    <template v-else-if="sichtbareZeilen.length === 0">
      <Leer
        titel="Noch keine Einträge"
        erklaerung="Sobald sich jemand anmeldet oder etwas geändert wird, steht es hier."
      />
    </template>

    <template v-else-if="gefiltert.length === 0">
      <Leer
        titel="Keine Treffer"
        :erklaerung="
          suche.trim()
            ? `Nichts passt zu „${suche}“.`
            : 'Kein Eintrag passt zur gewählten Art.'
        "
      >
        <template #aktion>
          <WpButton label="Filter leeren" @press="filterLeeren" />
        </template>
      </Leer>
    </template>

    <Karte
      v-else
      titel="Einträge"
      :unter="`${eintraege_text(gefiltert.length)} · neueste zuerst`"
    >
      <template #aktionen>
        <WpButton
          v-if="daten?.mehr"
          label="Nachladen"
          :icon="RefreshCw"
          size="sm"
          :state="nachladen ? 'busy' : 'default'"
          @press="weiter"
        />
        <Status v-else ton="leise" text="alles geladen" />
      </template>

      <div class="tabelle__rahmen">
        <table class="tabelle">
          <thead>
            <tr>
              <th scope="col">Zeit</th>
              <th scope="col">Art</th>
              <th scope="col">Wer</th>
              <th scope="col">Was</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(eintrag, i) in gefiltert" :key="`${eintrag.zeit}-${i}`">
              <td>
                <span :title="zeitpunkt(eintrag.zeit)">
                  {{ datum(eintrag.zeit) }}
                </span>
              </td>
              <td>
                <Status
                  :ton="art_ton(eintrag.art)"
                  :text="art_titel(eintrag.art)"
                />
              </td>
              <td>{{ eintrag.akteur }}</td>
              <td>
                {{ eintrag.text }}
                <span v-if="eintrag.anzahl > 1" class="pruefspur__zaehler">
                  · {{ eintrag.anzahl }}×
                </span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </Karte>
  </div>
</template>

<style scoped>
.pruefspur__zaehler {
  color: var(--wp-color-text-muted);
}

.seite {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-4);
}

.werkzeuge {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.werkzeuge__suche {
  flex: 1 1 calc(var(--wp-space-8) * 4);
  min-width: 0;
}

.werkzeuge__filter {
  display: flex;
  align-items: flex-end;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.hinweis {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.hinweis__zeichen {
  flex: none;
  width: var(--wp-font-size-md);
  height: var(--wp-font-size-md);
}

.tabelle__rahmen {
  overflow-x: auto;
}
</style>
