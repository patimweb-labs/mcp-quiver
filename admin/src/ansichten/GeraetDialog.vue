<script setup lang="ts">
// Ein Gerät ändern: Name, Gruppen. Sperren ist eine eigene Handlung auf der
// Geräteseite, damit man sie nicht aus Versehen mit einem Speichern umdreht.
import { computed, ref, watch } from "vue";
import {
  WpButton,
  WpDialog,
  WpInput,
  WpMultiselect,
} from "@weppelmann/components";

import { fehlertext, type Client } from "../api/client";
import type { GeraetStand, Gruppe, GruppeId } from "../api/typen";
import Feld from "../bausteine/Feld.vue";

const props = defineProps<{
  offen: boolean;
  stand: GeraetStand | null;
  gruppen: Gruppe[];
  client: Client;
}>();

const emit = defineEmits<{
  "update:offen": [boolean];
  gespeichert: [GeraetStand];
}>();

const name = ref("");
const gewaehlt = ref<GruppeId[]>([]);
const speichert = ref(false);
const speicherfehler = ref("");

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    name.value = props.stand?.geraet.name ?? "";
    gewaehlt.value = [...(props.stand?.geraet.gruppen ?? [])];
    speicherfehler.value = "";
  },
  { immediate: true },
);

const gruppenOptionen = computed(() =>
  [...props.gruppen]
    .sort((a, b) => a.titel.localeCompare(b.titel, "de", { sensitivity: "base" }))
    .map((gruppe) => ({
      value: gruppe.id,
      label: gruppe.titel,
      hint: gruppe.id,
    })),
);

async function speichern(): Promise<void> {
  if (!props.stand) return;
  speichert.value = true;
  speicherfehler.value = "";
  try {
    const gespeichert = await props.client.geraetSetzen(props.stand.geraet.id, {
      name: name.value.trim(),
      gruppen: gewaehlt.value,
      gesperrt: props.stand.geraet.gesperrt,
    });
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
    title="Gerät ändern"
    :description="
      stand
        ? `${stand.geraet.id} · ${stand.geraet.name || 'ohne Namen'}`
        : ''
    "
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div class="form">
      <Feld beschriftung="Name" hinweis="Der Anzeigename, z. B. der Rechnername.">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="name"
            @update:model-value="(wert) => (name = wert)"
          />
        </template>
      </Feld>

      <div class="auswahl">
        <WpMultiselect
          :model-value="gewaehlt"
          :options="gruppenOptionen"
          label="Gruppen"
          placeholder="Gruppen auswählen …"
          @update:model-value="(werte) => (gewaehlt = werte)"
        />
        <p class="hinweis">Mehrfachauswahl.</p>
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
