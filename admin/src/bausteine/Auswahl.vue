<script setup lang="ts">
// Eine Auswahl im Maß eines Eingabefelds – der eine Baustein für **alle**
// Auswahllisten der Verwaltung (Aufgabe 33).
//
// Die Liste liegt in einer eigenen Ebene am `body`, über dem Dialog: sie wird
// nicht abgeschnitten, macht den Dialog nicht höher und scrollt ihn nicht.
// Bedienung wie WAI-ARIA „combobox with listbox popup“: der Fokus bleibt auf
// dem Feld, die aktive Zeile wird über `aria-activedescendant` gemeldet.
// Escape schließt **nur** die Liste.
import { computed, ref, watch } from "vue";
import { IconCheck, IconChevronDown } from "@weppelmann/icons";
import type { ListOption } from "@weppelmann/components";

import {
  erste_aktive,
  letzte_aktive,
  naechste_aktive,
  tippsuche,
} from "../logik/liste";
import { naechste_kennung, useAuswahlEbene } from "./auswahl-ebene";

type Zustand = "default" | "invalid" | "disabled";

const props = withDefaults(
  defineProps<{
    modelValue: string | null;
    options: ListOption[];
    label?: string;
    placeholder?: string;
    state?: Zustand;
    /** Kennung des Auslösers; verbindet ihn mit der Beschriftung im Feld. */
    id?: string;
    /** Kennung eines Hinweises (`aria-describedby`). */
    beschrieben?: string;
  }>(),
  { placeholder: "Auswählen …", state: "default" },
);
const emit = defineEmits<{ "update:modelValue": [wert: string | null] }>();

const eigene = naechste_kennung("auswahl");
const kennung = props.id ?? eigene;
const listen_id = `${kennung}-liste`;
const option_id = (i: number): string => `${kennung}-opt-${i}`;

const { offen, platz, stil, anker_setzen, ebene_setzen, oeffnen, schliessen, umschalten } =
  useAuswahlEbene();
const aktiv = ref(-1);

const gewaehlt = computed(() => props.options.findIndex((o) => o.value === props.modelValue));
const anzeige = computed(() => (gewaehlt.value >= 0 ? props.options[gewaehlt.value].label : ""));

function aufmachen(): void {
  offen_werte_setzen();
  oeffnen();
}

function offen_werte_setzen(): void {
  aktiv.value = gewaehlt.value >= 0 ? gewaehlt.value : erste_aktive(props.options);
}

function waehlen(i: number): void {
  const option = props.options[i];
  if (!option || option.disabled) return;
  emit("update:modelValue", option.value);
  schliessen();
}

function taste(event: KeyboardEvent): void {
  if (props.state === "disabled") return;
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      if (!offen.value) aufmachen();
      else aktiv.value = naechste_aktive(props.options, aktiv.value, 1);
      return;
    case "ArrowUp":
      event.preventDefault();
      if (!offen.value) aufmachen();
      else aktiv.value = naechste_aktive(props.options, aktiv.value, -1);
      return;
    case "Home":
      if (!offen.value) return;
      event.preventDefault();
      aktiv.value = erste_aktive(props.options);
      return;
    case "End":
      if (!offen.value) return;
      event.preventDefault();
      aktiv.value = letzte_aktive(props.options);
      return;
    case "Enter":
    case " ":
      event.preventDefault();
      if (!offen.value) aufmachen();
      else waehlen(aktiv.value);
      return;
    case "Escape":
      // Nur die Liste schließen; der Dialog bleibt offen. Ohne offene Liste
      // gehört Escape dem Dialog.
      if (!offen.value) return;
      event.preventDefault();
      event.stopPropagation();
      schliessen();
      return;
    case "Tab":
      schliessen();
      return;
    default:
      if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
        if (!offen.value) aufmachen();
        const treffer = tippsuche(props.options, event.key, Math.max(aktiv.value, 0));
        if (treffer >= 0) aktiv.value = treffer;
      }
  }
}

// Ein neuer Wert von außen (etwa nach dem Neuladen) schließt die Liste nicht.
watch(
  () => props.options,
  () => {
    if (offen.value) offen_werte_setzen();
  },
);
</script>

<template>
  <div class="auswahl" :class="`is-${state}`">
    <span v-if="label" :id="`${kennung}-label`" class="auswahl__label">{{ label }}</span>
    <button
      :ref="anker_setzen"
      type="button"
      class="auswahl__feld"
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
      @click="umschalten"
      @keydown="taste"
    >
      <span :class="anzeige ? 'auswahl__wert' : 'auswahl__platzhalter'">
        {{ anzeige || placeholder }}
      </span>
      <IconChevronDown class="auswahl__zeichen" aria-hidden="true" />
    </button>

    <Teleport to="body">
      <div
        v-if="offen && platz"
        :ref="ebene_setzen"
        class="auswahl-ebene"
        :data-auswahl="kennung"
        :style="stil"
      >
        <ul
          :id="listen_id"
          class="auswahl-ebene__liste"
          role="listbox"
          :aria-labelledby="label ? `${kennung}-label` : undefined"
        >
          <li
            v-for="(option, i) in options"
            :id="option_id(i)"
            :key="option.value"
            class="auswahl-ebene__eintrag"
            :class="{
              'is-aktiv': i === aktiv,
              'is-gewaehlt': i === gewaehlt,
              'is-gesperrt': option.disabled,
            }"
            role="option"
            :aria-selected="i === gewaehlt"
            :aria-disabled="option.disabled || undefined"
            @mousedown.prevent="waehlen(i)"
            @mouseenter="!option.disabled && (aktiv = i)"
          >
            <IconCheck
              v-if="i === gewaehlt"
              class="auswahl-ebene__haken"
              aria-hidden="true"
            />
            <span class="auswahl-ebene__text">{{ option.label }}</span>
            <span v-if="option.hint" class="auswahl-ebene__hinweis">{{ option.hint }}</span>
          </li>
          <li v-if="options.length === 0" class="auswahl-ebene__leer">Keine Einträge</li>
        </ul>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.auswahl {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.auswahl__label {
  font-size: var(--wp-font-size-sm);
  color: var(--wp-color-text-muted);
}
.auswahl__feld {
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
.auswahl__feld:focus-visible {
  outline: var(--wp-size-focus) solid var(--wp-color-focus-ring);
  outline-offset: 0;
}
.auswahl.is-invalid .auswahl__feld {
  border-color: var(--wp-color-status-error);
}
.auswahl.is-disabled {
  opacity: 0.55;
}
.auswahl.is-disabled .auswahl__feld {
  cursor: not-allowed;
}
.auswahl__platzhalter {
  color: var(--wp-color-text-faint);
}
.auswahl__zeichen {
  flex: none;
  width: var(--wp-font-size-md);
  height: var(--wp-font-size-md);
  color: var(--wp-color-text-faint);
}
</style>
