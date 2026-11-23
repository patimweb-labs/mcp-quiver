<script setup lang="ts">
// Ein Satz: Titel, Priorität und die Einträge mit ihren Server-Werten.
// Nur Server-Felder bekommen hier eine Eingabe – Client- und Geheimfelder
// gehören ans Gerät (SPEC.md §6).
import { computed, ref, watch } from "vue";
import { Trash2 } from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpIconButton,
  WpInput,
  WpSelect,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { fehlertext, type Client } from "../api/client";
import type { Eintrag, Satz, SatzEintrag } from "../api/typen";
import Feld from "../bausteine/Feld.vue";
import {
  server_felder,
  sortiert_nach_titel,
} from "../logik/eintrag";
import {
  neuer_satz,
  satz_eintrag_entfernen,
  satz_eintrag_hinzufuegen,
  satz_pruefen,
  satz_saeubern,
  satz_wert_setzen,
} from "../logik/satz";

const props = defineProps<{
  offen: boolean;
  satz: Satz | null;
  neu: boolean;
  eintraege: Eintrag[];
  client: Client;
}>();

const emit = defineEmits<{
  "update:offen": [boolean];
  gespeichert: [Satz];
}>();

const entwurf = ref<Satz>(neuer_satz());
const fehler = ref<Record<string, string>>({});
const speicherfehler = ref("");
const speichert = ref(false);
const auswahl = ref<string | null>(null);

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    entwurf.value = props.satz ? tief_kopie(props.satz) : neuer_satz();
    fehler.value = {};
    speicherfehler.value = "";
    auswahl.value = null;
  },
  { immediate: true },
);

const nach_kennung = computed(
  () => new Map(props.eintraege.map((eintrag) => [eintrag.id, eintrag])),
);

/** Die Zeilen des Satzes mit ihrem Eintrag; unbekannte Kennungen fallen weg. */
const zeilen = computed(() => {
  const aus: { zeile: SatzEintrag; eintrag: Eintrag }[] = [];
  for (const zeile of entwurf.value.eintraege) {
    const eintrag = nach_kennung.value.get(zeile.eintrag);
    if (eintrag) aus.push({ zeile, eintrag });
  }
  return aus;
});

const hinzufuegbar = computed(() => {
  const vorhanden = new Set(entwurf.value.eintraege.map((zeile) => zeile.eintrag));
  return sortiert_nach_titel(
    props.eintraege.filter((eintrag) => !vorhanden.has(eintrag.id)),
  ).map((eintrag) => ({
    value: eintrag.id,
    label: eintrag.titel,
    hint: eintrag.id,
  }));
});

function prioritaetSetzen(wert: string): void {
  const zahl = Number.parseInt(wert, 10);
  entwurf.value.prioritaet = Number.isNaN(zahl) ? 0 : zahl;
}

const prioritaet = computed(() => String(entwurf.value.prioritaet));

function hinzufuegen(wert: string | null): void {
  if (!wert) return;
  entwurf.value = satz_eintrag_hinzufuegen(entwurf.value, wert);
  auswahl.value = null;
}

function entfernen(eintrag: string): void {
  entwurf.value = satz_eintrag_entfernen(entwurf.value, eintrag);
}

function wertSetzen(eintrag: string, feld: string, wert: string): void {
  entwurf.value = satz_wert_setzen(entwurf.value, eintrag, feld, wert);
}

async function speichern(): Promise<void> {
  const sauber = satz_saeubern(entwurf.value);
  fehler.value = satz_pruefen(sauber);
  if (Object.keys(fehler.value).length > 0) return;

  speichert.value = true;
  speicherfehler.value = "";
  try {
    const gespeichert = await props.client.satzSetzen(sauber);
    emit("gespeichert", gespeichert);
    emit("update:offen", false);
  } catch (fehlerWert) {
    speicherfehler.value = fehlertext(fehlerWert);
  } finally {
    speichert.value = false;
  }
}
</script>

<template>
  <WpDialog
    :open="offen"
    :title="neu ? 'Neuer Satz' : 'Satz bearbeiten'"
    description="Eine Sammlung von Einträgen mit vorbelegten Server-Werten."
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div class="form">
      <Feld beschriftung="Kennung" :fehler="fehler.id">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="entwurf.id"
            :state="fehler.id ? 'invalid' : 'default'"
            placeholder="basis"
            @update:model-value="(wert) => (entwurf.id = wert)"
          />
        </template>
      </Feld>

      <Feld beschriftung="Titel" :fehler="fehler.titel">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="entwurf.titel"
            :state="fehler.titel ? 'invalid' : 'default'"
            placeholder="Grundausstattung"
            @update:model-value="(wert) => (entwurf.titel = wert)"
          />
        </template>
      </Feld>

      <Feld
        beschriftung="Priorität"
        :fehler="fehler.prioritaet"
        hinweis="Bei widersprüchlichen Werten gewinnt die höhere Zahl."
      >
        <template #default="{ id }">
          <WpInput
            :id="id"
            type="number"
            :model-value="prioritaet"
            :state="fehler.prioritaet ? 'invalid' : 'default'"
            @update:model-value="prioritaetSetzen"
          />
        </template>
      </Feld>

      <div class="eintraege">
        <div class="eintraege__kopf">
          <span class="eintraege__titel">Einträge</span>
        </div>

        <p v-if="zeilen.length === 0" class="blass">
          Noch keine Einträge in diesem Satz.
        </p>

        <fieldset
          v-for="z in zeilen"
          :key="z.zeile.eintrag"
          class="satzzeile"
        >
          <legend class="satzzeile__titel">
            {{ z.eintrag.titel }}
            <span class="mono blass">{{ z.eintrag.id }}</span>
          </legend>
          <div class="satzzeile__kopf">
            <span class="blass">
              {{ server_felder(z.eintrag).length }} Server-Feld(er)
            </span>
            <WpIconButton
              :icon="Trash2"
              label="Eintrag aus dem Satz nehmen"
              size="sm"
              variant="ghost"
              @press="entfernen(z.zeile.eintrag)"
            />
          </div>

          <p v-if="server_felder(z.eintrag).length === 0" class="blass">
            Dieser Eintrag hat keine Server-Felder.
          </p>

          <Feld
            v-for="feld in server_felder(z.eintrag)"
            :key="feld.name"
            :beschriftung="feld.name"
            :hinweis="feld.beschreibung || undefined"
          >
            <template #default="{ id }">
              <WpInput
                :id="id"
                :model-value="z.zeile.werte[feld.name] ?? ''"
                :placeholder="feld.standard ?? ''"
                @update:model-value="
                  (wert) => wertSetzen(z.zeile.eintrag, feld.name, wert)
                "
              />
            </template>
          </Feld>
        </fieldset>

        <WpSelect
          :model-value="auswahl"
          :options="hinzufuegbar"
          placeholder="Eintrag hinzufügen …"
          label="Eintrag hinzufügen"
          @update:model-value="hinzufuegen"
        />
      </div>

      <p v-if="speicherfehler" class="fehler" role="alert">{{ speicherfehler }}</p>
    </div>

    <template #actions>
      <WpButton label="Abbrechen" @press="emit('update:offen', false)" />
      <WpButton
        label="Speichern"
        variant="primary"
        :state="speichert ? 'busy' : 'default'"
        @press="speichern"
      />
    </template>
  </WpDialog>
</template>

<style scoped>
.form {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  max-height: calc(var(--wp-space-8) * 9);
  overflow-y: auto;
  padding-right: var(--wp-space-1);
}

.eintraege {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
  padding-top: var(--wp-space-3);
}

.eintraege__kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.eintraege__titel {
  font-size: var(--wp-font-size-lg);
  font-weight: var(--wp-font-weight-semibold);
}

.satzzeile {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  margin: 0;
  padding: var(--wp-space-3);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  background: var(--wp-color-bg-raised);
}

.satzzeile__titel {
  display: flex;
  align-items: baseline;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
  padding-inline: var(--wp-space-1);
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-medium);
}

.satzzeile__kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-2);
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
