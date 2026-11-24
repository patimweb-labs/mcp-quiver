<script setup lang="ts">
// Gerät einbinden: Gruppen wählen, Code erzeugen, Code groß zeigen. Der Code
// gilt nur eine Weile (oder ohne Ablauf, bis er eingelöst ist); die Ablaufzeit
// steht daneben (nicht nur im Kopf).
import { computed, ref, watch } from "vue";
import { RefreshCw } from "lucide-vue-next";
import {
  WpButton,
  WpCheckbox,
  WpDialog,
  WpInput,
  WpMultiselect,
} from "@weppelmann/components";

import { fehlertext, type Client } from "../api/client";
import type { Code, Gruppe, GruppeId } from "../api/typen";
import Feld from "../bausteine/Feld.vue";
import Kopierbar from "../bausteine/Kopierbar.vue";
import Status from "../bausteine/Status.vue";
import { datum, restzeit } from "../logik/format";

const props = defineProps<{
  offen: boolean;
  gruppen: Gruppe[];
  client: Client;
}>();

const emit = defineEmits<{ "update:offen": [boolean] }>();

const gewaehlt = ref<GruppeId[]>([]);
const minuten = ref("60");
const unbegrenzt = ref(false);
const code = ref<Code | null>(null);
const fehler = ref("");
const laedt = ref(false);

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    gewaehlt.value = [];
    minuten.value = "60";
    unbegrenzt.value = false;
    code.value = null;
    fehler.value = "";
  },
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

const gruppenNamen = computed(() => {
  const nach = new Map(props.gruppen.map((gruppe) => [gruppe.id, gruppe.titel]));
  return gewaehlt.value.map((id) => nach.get(id) ?? id).join(", ");
});

async function erzeugen(): Promise<void> {
  const zahl = Number.parseInt(minuten.value, 10);
  laedt.value = true;
  fehler.value = "";
  try {
    code.value = await props.client.codeAnlegen({
      gruppen: [...gewaehlt.value],
      minuten: Number.isNaN(zahl) || zahl <= 0 ? null : zahl,
      unbegrenzt: unbegrenzt.value,
    });
  } catch (fehlerWert) {
    fehler.value = fehlertext(fehlerWert);
  } finally {
    laedt.value = false;
  }
}

function neu(): void {
  code.value = null;
  fehler.value = "";
}
</script>

<template>
  <WpDialog
    :open="offen"
    title="Gerät einbinden"
    description="Der Code gilt einmal; das Gerät landet in den gewählten Gruppen."
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div v-if="code" class="code">
      <p class="code__wert">{{ code.code }}</p>
      <Kopierbar :wert="code.code" kopieren="Code kopieren" />
      <p class="code__zeit">
        <Status
          v-if="code.gueltig_bis"
          ton="warnung"
          :text="`${restzeit(code.gueltig_bis)} · bis ${datum(code.gueltig_bis)}`"
        />
        <Status v-else ton="warnung" text="Ohne Ablauf · gilt, bis ein Gerät ihn einlöst" />
      </p>
      <p v-if="gruppenNamen" class="blass">Gruppen: {{ gruppenNamen }}</p>
      <p v-else class="blass">Ohne Gruppen: das Gerät bekommt zunächst nichts.</p>
    </div>

    <div v-else class="form">
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

      <Feld beschriftung="Gültigkeit" hinweis="In Minuten; Standard 60.">
        <template #default="{ id }">
          <WpInput
            :id="id"
            type="number"
            :disabled="unbegrenzt"
            :model-value="minuten"
            @update:model-value="(wert) => (minuten = wert)"
          />
        </template>
      </Feld>
      <WpCheckbox
        :model-value="unbegrenzt"
        label="Ohne Ablauf (gilt, bis ein Gerät ihn einlöst)"
        @update:model-value="(wert) => (unbegrenzt = wert)"
      />

      <p v-if="fehler" class="fehler" role="alert">{{ fehler }}</p>
    </div>

    <template #actions>
      <WpButton label="Schließen" @press="emit('update:offen', false)" />
      <WpButton
        v-if="code"
        label="Neuen Code erzeugen"
        :icon="RefreshCw"
        @press="neu"
      />
      <WpButton
        v-else
        label="Code erzeugen"
        variant="primary"
        :state="laedt ? 'busy' : 'default'"
        @press="erzeugen"
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

.code {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--wp-space-2);
  padding: var(--wp-space-4);
  background: var(--wp-color-bg-input);
  border: var(--wp-size-border) solid var(--wp-color-border-strong);
}

.code__wert {
  font-family: var(--wp-font-family-mono);
  font-size: var(--wp-font-size-2xl);
  line-height: var(--wp-font-line-tight);
  overflow-wrap: anywhere;
}

.code__zeit {
  display: flex;
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
