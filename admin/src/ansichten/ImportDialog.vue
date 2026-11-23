<script setup lang="ts">
// Importieren: server.json einfügen oder eine Datei wählen. Geprüft wird
// zuerst hier (JSON), dann auf dem Server (Bibliotheksregeln).
import { ref, watch } from "vue";
import { Upload } from "lucide-vue-next";
import { WpButton, WpDialog, WpTextarea } from "@weppelmann/components";

import { fehlertext, type Client } from "../api/client";
import type { Eintrag } from "../api/typen";
import { server_json_lesen } from "../logik/serverjson";

const props = defineProps<{ offen: boolean; client: Client }>();
const emit = defineEmits<{
  "update:offen": [boolean];
  importiert: [Eintrag];
}>();

const inhalt = ref("");
const fehler = ref("");
const laedt = ref(false);
const dateiname = ref("");

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    inhalt.value = "";
    fehler.value = "";
    dateiname.value = "";
  },
);

async function dateiLesen(ereignis: Event): Promise<void> {
  const eingabe = ereignis.target as HTMLInputElement;
  const datei = eingabe.files?.[0];
  if (!datei) return;
  dateiname.value = datei.name;
  try {
    inhalt.value = await datei.text();
    fehler.value = "";
  } catch {
    fehler.value = "Die Datei konnte nicht gelesen werden.";
  }
}

async function importieren(): Promise<void> {
  const gelesen = server_json_lesen(inhalt.value);
  if (gelesen.art === "fehler") {
    fehler.value = gelesen.text;
    return;
  }
  laedt.value = true;
  fehler.value = "";
  try {
    const eintrag = await props.client.eintragImportieren(gelesen.wert);
    emit("importiert", eintrag);
    emit("update:offen", false);
  } catch (fehlerWert) {
    fehler.value = fehlertext(fehlerWert);
  } finally {
    laedt.value = false;
  }
}
</script>

<template>
  <WpDialog
    :open="offen"
    title="server.json importieren"
    description="Der Eintrag wird aus einer MCP-Server-Beschreibung angelegt."
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div class="import">
      <label class="import__datei">
        <span>Datei wählen</span>
        <input type="file" accept=".json,application/json" @change="dateiLesen" />
      </label>
      <p v-if="dateiname" class="blass">{{ dateiname }}</p>

      <label class="import__text">
        <span>oder einfügen</span>
        <WpTextarea
          v-model="inhalt"
          :rows="10"
          placeholder='{ "mcpServers": { … } }'
        />
      </label>

      <p v-if="fehler" class="import__fehler" role="alert">{{ fehler }}</p>
    </div>

    <template #actions>
      <WpButton label="Abbrechen" @press="emit('update:offen', false)" />
      <WpButton
        label="Importieren"
        variant="primary"
        :icon="Upload"
        :state="laedt ? 'busy' : 'default'"
        @press="importieren"
      />
    </template>
  </WpDialog>
</template>

<style scoped>
.import {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.import__datei,
.import__text {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.import__datei input {
  color: var(--wp-color-text-default);
  font: inherit;
  font-size: var(--wp-font-size-md);
}

.import__fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
