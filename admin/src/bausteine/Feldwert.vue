<script setup lang="ts">
// Der Wert eines Felds, nach seiner Art (Aufgabe 16). Dieselben Arten wie im
// Client-Fenster: Text, Passwort, Zahl, Schalter, Adresse, Auswahl.
import { computed } from "vue";
import { WpCheckbox, WpInput } from "@weppelmann/components";

import Auswahl from "./Auswahl.vue";

import type { Feld } from "../api/typen";
import {
  feld_art,
  feld_optionen,
  feld_titel,
  schalter_an,
} from "../logik/feld";

const props = defineProps<{
  feld: Feld;
  modelValue: string;
  /** Kennung des Eingabefelds, aus `Feld.vue`. */
  id?: string;
  /** Kennung des Hinweises, aus `Feld.vue`. */
  beschrieben?: string;
  placeholder?: string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const art = computed(() => feld_art(props.feld));
const auswahl = computed(() =>
  feld_optionen(props.feld).map((option) => ({
    value: option.wert,
    label: option.anzeige,
  })),
);

function zahlSetzen(wert: string): void {
  emit("update:modelValue", wert.replace(/\D/g, ""));
}
</script>

<template>
  <WpCheckbox
    v-if="art === 'schalter'"
    :model-value="schalter_an(modelValue)"
    :label="feld_titel(feld)"
    @update:model-value="(an) => emit('update:modelValue', an ? 'an' : 'aus')"
  />
  <Auswahl
    v-else-if="typeof art !== 'string'"
    :id="id"
    :beschrieben="beschrieben"
    :model-value="modelValue.length > 0 ? modelValue : null"
    :options="auswahl"
    placeholder="Auswählen …"
    @update:model-value="(wert) => emit('update:modelValue', wert ?? '')"
  />
  <WpInput
    v-else
    :id="id"
    :type="art === 'passwort' ? 'password' : 'text'"
    :model-value="modelValue"
    :placeholder="placeholder"
    @update:model-value="art === 'zahl' ? zahlSetzen($event) : emit('update:modelValue', $event)"
  />
</template>
