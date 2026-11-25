<script setup lang="ts">
// Mehrfachauswahl – derselbe Baustein wie `Auswahl.vue`, nur mit Haken und
// offener Liste (Aufgabe 33). Die Liste liegt ebenfalls in einer eigenen Ebene
// am `body`, also über dem Dialog, und wird nie höher als der freie Platz.
//
// Mit `suchbar` filtert Tippen die Einträge; der Fokus bleibt dabei auf dem
// Feld (kein zweites Eingabefeld, keine Fokusfalle). Backspace löscht die
// Suche, Escape schließt erst die Suche und dann die Liste.
import { computed, ref, watch } from "vue";
import { IconCheck, IconChevronDown } from "@weppelmann/icons";
import type { ListOption } from "@weppelmann/components";

import {
  erste_aktive,
  filtern,
  letzte_aktive,
  naechste_aktive,
} from "../logik/liste";
import { naechste_kennung, useAuswahlEbene } from "./auswahl-ebene";

type Zustand = "default" | "invalid" | "disabled";

const props = withDefaults(
  defineProps<{
    modelValue: string[];
    options: ListOption[];
    label?: string;
    placeholder?: string;
    state?: Zustand;
    /** Tippen filtert die Einträge. */
    suchbar?: boolean;
    /** Kennung des Auslösers; verbindet ihn mit der Beschriftung im Feld. */
    id?: string;
    /** Kennung eines Hinweises (`aria-describedby`). */
    beschrieben?: string;
  }>(),
  { placeholder: "Auswählen …", state: "default", suchbar: false },
);
const emit = defineEmits<{ "update:modelValue": [werte: string[]] }>();

const eigene = naechste_kennung("mehrfach");
const kennung = props.id ?? eigene;
const listen_id = `${kennung}-liste`;
const option_id = (i: number): string => `${kennung}-opt-${i}`;

const { offen, platz, stil, anker_setzen, ebene_setzen, oeffnen, schliessen } =
  useAuswahlEbene();
const aktiv = ref(-1);
const suche = ref("");

const gewaehlt = computed(() => new Set(props.modelValue));
const sichtbar = computed(() =>
  props.suchbar ? filtern(props.options, suche.value) : props.options,
);
const zusammenfassung = computed(() => {
  const anzahl = props.modelValue.length;
  if (anzahl === 0) return "";
  if (anzahl === 1) {
    return props.options.find((option) => option.value === props.modelValue[0])?.label ?? "1 ausgewählt";
  }
  return `${anzahl} ausgewählt`;
});

function aufmachen(): void {
  suche.value = "";
  aktiv.value = erste_aktive(sichtbar.value);
  oeffnen();
}

function umschalten_eintrag(i: number): void {
  const option = sichtbar.value[i];
  if (!option || option.disabled) return;
  const menge = new Set(props.modelValue);
  if (menge.has(option.value)) menge.delete(option.value);
  else menge.add(option.value);
  // Die Reihenfolge folgt den Optionen, nicht dem Klicken.
  emit(
    "update:modelValue",
    props.options.filter((eintrag) => menge.has(eintrag.value)).map((eintrag) => eintrag.value),
  );
}

function taste(event: KeyboardEvent): void {
  if (props.state === "disabled") return;
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      if (!offen.value) aufmachen();
      else aktiv.value = naechste_aktive(sichtbar.value, aktiv.value, 1);
      return;
    case "ArrowUp":
      event.preventDefault();
      if (!offen.value) aufmachen();
      else aktiv.value = naechste_aktive(sichtbar.value, aktiv.value, -1);
      return;
    case "Home":
      if (!offen.value) return;
      event.preventDefault();
      aktiv.value = erste_aktive(sichtbar.value);
      return;
    case "End":
      if (!offen.value) return;
      event.preventDefault();
      aktiv.value = letzte_aktive(sichtbar.value);
      return;
    case "Enter":
    case " ":
      event.preventDefault();
      if (!offen.value) aufmachen();
      else umschalten_eintrag(aktiv.value);
      return;
    case "Backspace":
      if (!offen.value || suche.value.length === 0) return;
      event.preventDefault();
      suche.value = suche.value.slice(0, -1);
      aktiv.value = erste_aktive(sichtbar.value);
      return;
    case "Escape":
      if (!offen.value) return;
      event.preventDefault();
      event.stopPropagation();
      if (suche.value.length > 0) {
        suche.value = "";
        aktiv.value = erste_aktive(sichtbar.value);
      } else {
        schliessen();
      }
      return;
    case "Tab":
      schliessen();
      return;
    default:
      if (
        props.suchbar &&
        event.key.length === 1 &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey
      ) {
        event.preventDefault();
        if (!offen.value) aufmachen();
        suche.value += event.key;
        aktiv.value = erste_aktive(sichtbar.value);
      }
  }
}

// Eine geänderte Auswahl von außen lässt die Liste offen; gesperrte oder
// verschwundene Einträge dürfen die aktive Zeile nicht verrutschen lassen.
watch(
  () => props.options,
  () => {
    if (!offen.value) return;
    aktiv.value = Math.min(aktiv.value, sichtbar.value.length - 1);
  },
);
</script>

<template>
  <div class="mehrfach" :class="`is-${state}`">
    <span v-if="label" :id="`${kennung}-label`" class="mehrfach__label">{{ label }}</span>
    <button
      :ref="anker_setzen"
      type="button"
      class="mehrfach__feld"
      :id="id"
      role="combobox"
      aria-haspopup="listbox"
      :aria-describedby="beschrieben"
      :aria-expanded="offen"
      :aria-controls="listen_id"
      :aria-labelledby="label ? `${kennung}-label` : undefined"
      :aria-activedescendant="offen && aktiv >= 0 ? option_id(aktiv) : undefined"
      :aria-invalid="state === 'invalid' || undefined"
      :disabled="state === 'disabled'"
      @click="offen ? schliessen() : aufmachen()"
      @keydown="taste"
    >
      <span :class="zusammenfassung ? 'mehrfach__wert' : 'mehrfach__platzhalter'">
        {{ zusammenfassung || placeholder }}
      </span>
      <IconChevronDown class="mehrfach__zeichen" aria-hidden="true" />
    </button>

    <Teleport to="body">
      <div
        v-if="offen && platz"
        :ref="ebene_setzen"
        class="auswahl-ebene"
        :data-auswahl="kennung"
        :style="stil"
      >
        <p v-if="suchbar && suche.length > 0" class="auswahl-ebene__suche" aria-live="polite">
          Suche: {{ suche }}
        </p>
        <ul
          :id="listen_id"
          class="auswahl-ebene__liste"
          role="listbox"
          aria-multiselectable="true"
          :aria-labelledby="label ? `${kennung}-label` : undefined"
        >
          <li
            v-for="(option, i) in sichtbar"
            :id="option_id(i)"
            :key="option.value"
            class="auswahl-ebene__eintrag"
            :class="{
              'is-aktiv': i === aktiv,
              'is-gesperrt': option.disabled,
            }"
            role="option"
            :aria-selected="gewaehlt.has(option.value)"
            :aria-disabled="option.disabled || undefined"
            @mousedown.prevent="umschalten_eintrag(i)"
            @mouseenter="!option.disabled && (aktiv = i)"
          >
            <span
              class="auswahl-ebene__kasten"
              :class="{ 'is-an': gewaehlt.has(option.value) }"
              aria-hidden="true"
            >
              <IconCheck v-if="gewaehlt.has(option.value)" class="auswahl-ebene__haken" />
            </span>
            <span class="auswahl-ebene__text">{{ option.label }}</span>
            <span v-if="option.hint" class="auswahl-ebene__hinweis">{{ option.hint }}</span>
          </li>
          <li v-if="sichtbar.length === 0" class="auswahl-ebene__leer">
            {{ suchbar && suche.length > 0 ? `Nichts passt zu „${suche}“` : "Keine Einträge" }}
          </li>
        </ul>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.mehrfach {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.mehrfach__label {
  font-size: var(--wp-font-size-sm);
  color: var(--wp-color-text-muted);
}
.mehrfach__feld {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-2);
  width: 100%;
  min-height: var(--wp-size-control-md);
  padding: 0 var(--wp-space-2);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  background: var(--wp-color-bg-input);
  color: var(--wp-color-text-default);
  font: inherit;
  font-size: var(--wp-font-size-md);
  text-align: left;
  cursor: pointer;
}
.mehrfach__feld:focus-visible {
  outline: var(--wp-size-focus) solid var(--wp-color-focus-ring);
  outline-offset: 0;
}
.mehrfach.is-invalid .mehrfach__feld {
  border-color: var(--wp-color-status-error);
}
.mehrfach.is-disabled {
  opacity: 0.55;
}
.mehrfach.is-disabled .mehrfach__feld {
  cursor: not-allowed;
}
.mehrfach__platzhalter {
  color: var(--wp-color-text-faint);
}
.mehrfach__zeichen {
  flex: none;
  width: var(--wp-font-size-md);
  height: var(--wp-font-size-md);
  color: var(--wp-color-text-faint);
}
</style>
