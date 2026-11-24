<script setup lang="ts">
// Eine Gruppe: Titel und die Sätze, die ihre Geräte bekommen. Stammt sie aus
// Entra ID (Aufgabe 23), ist ihre Kennung die Objekt-Id und nicht änderbar;
// Titel und Sätze gehören dem Admin.
import { computed, ref, watch } from "vue";
import {
  WpButton,
  WpDialog,
  WpInput,
  WpMultiselect,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { ApiFehler, fehlertext, type Client } from "../api/client";
import type { Gruppe, Satz } from "../api/typen";
import Feld from "../bausteine/Feld.vue";
import { ist_entra_gruppe, ohne_satz } from "../logik/ausweis";
import {
  gruppe_pruefen,
  gruppe_saeubern,
  neue_gruppe,
  saetze_sortiert,
} from "../logik/satz";

const props = defineProps<{
  offen: boolean;
  gruppe: Gruppe | null;
  neu: boolean;
  satze: Satz[];
  client: Client;
}>();

const emit = defineEmits<{
  "update:offen": [boolean];
  neuladen: [];
  gespeichert: [Gruppe];
}>();

const entwurf = ref<Gruppe>(neue_gruppe());
const fehler = ref<Record<string, string>>({});
const speicherfehler = ref("");
const konflikt = ref(false);
const speichert = ref(false);

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    entwurf.value = props.gruppe
      ? tief_kopie(props.gruppe)
      : neue_gruppe();
    fehler.value = {};
    speicherfehler.value = "";
    konflikt.value = false;
  },
  { immediate: true },
);

const satzOptionen = computed(() =>
  saetze_sortiert(props.satze).map((satz) => ({
    value: satz.id,
    label: satz.titel,
    hint: `${satz.id} · Priorität ${satz.prioritaet}`,
  })),
);

async function speichern(): Promise<void> {
  const sauber = gruppe_saeubern(entwurf.value);
  fehler.value = gruppe_pruefen(sauber);
  if (Object.keys(fehler.value).length > 0) return;

  speichert.value = true;
  speicherfehler.value = "";
  try {
    const gespeichert = await props.client.gruppeSetzen(sauber);
    emit("gespeichert", gespeichert);
    emit("update:offen", false);
  } catch (fehlerWert) {
    konflikt.value = fehlerWert instanceof ApiFehler && fehlerWert.status === 409;
    speicherfehler.value = konflikt.value
      ? "Inzwischen geändert – bitte neu laden."
      : fehlertext(fehlerWert);
  } finally {
    speichert.value = false;
  }
}

/** Nach einem 409: die Liste neu holen und den Dialog schließen. */
function neu_laden(): void {
  emit("neuladen");
  emit("update:offen", false);
}
</script>

<template>
  <WpDialog
    :open="offen"
    :title="neu ? 'Neue Gruppe' : 'Gruppe bearbeiten'"
    description="Geräte in dieser Gruppe bekommen die zugewiesenen Sätze."
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div class="form">
      <Feld beschriftung="Kennung" :fehler="fehler.id">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="entwurf.id"
            :state="
              !neu && entwurf.entra.length > 0
                ? 'disabled'
                : fehler.id
                  ? 'invalid'
                  : 'default'
            "
            placeholder="team-entwicklung"
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
            placeholder="Entwicklung"
            @update:model-value="(wert) => (entwurf.titel = wert)"
          />
        </template>
      </Feld>

      <p v-if="!neu && ist_entra_gruppe(entwurf)" class="hinweis">
        Diese Gruppe stammt aus Entra ID. Kennung und Mitglieder pflegt Entra;
        Titel und Sätze gehören Ihnen.
      </p>

      <div class="auswahl">
        <WpMultiselect
          :model-value="entwurf.saetze"
          :options="satzOptionen"
          label="Sätze"
          placeholder="Sätze auswählen …"
          @update:model-value="(werte) => (entwurf.saetze = werte)"
        />
        <p class="hinweis">Mehrfachauswahl; die Reihenfolge folgt der Priorität.</p>
        <p v-if="ohne_satz(entwurf)" class="hinweis">
          Noch kein Satz – diese Gruppe gibt noch keine Tools.
        </p>
      </div>

      <p v-if="speicherfehler" class="fehler" role="alert">{{ speicherfehler }}</p>
    </div>

    <template #actions>
      <WpButton v-if="konflikt" label="Neu laden" @press="neu_laden" />
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

.auswahl {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  padding-block: var(--wp-space-1);
}

.hinweis {
  color: var(--wp-color-text-faint);
  font-size: var(--wp-font-size-sm);
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
