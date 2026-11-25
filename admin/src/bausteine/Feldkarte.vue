<script setup lang="ts">
// Eine Feldkarte im Eintrag-Formular: Name, Beschreibung, Art, Titel, Quelle,
// Pflicht, Standard und der Hilfe-Link (Aufgaben 13 und 16). Dieselbe Karte
// trägt die normalen Felder und die Felder einer Zugangsliste (Aufgabe 14) –
// nur die erlaubten Quellen unterscheiden sich.
import { Trash2 } from "lucide-vue-next";
import {
  WpCheckbox,
  WpIconButton,
  WpInput,
  WpTextarea,
} from "@weppelmann/components";

import type { Feld as FeldDaten, Quelle } from "../api/typen";
import {
  FELD_ARTEN,
  feld_art_aus_name,
  feld_art_name,
  feld_art_text,
  feld_optionen,
  optionen_als_text,
  optionen_aus_text,
} from "../logik/feld";
import Auswahl from "./Auswahl.vue";
import Feld from "./Feld.vue";

const props = defineProps<{
  /** Das Feld selbst; die Karte schreibt direkt hinein. */
  feld: FeldDaten;
  /** Nummer für die Überschrift (1-basiert). */
  nummer: number;
  /** Präfix der Fehlerschlüssel, z. B. `feld` oder `mehrfach.feld`. */
  praefix: string;
  /** Alle Fehler des Entwurfs; die Karte liest ihre eigenen heraus. */
  fehler: Record<string, string>;
  /** Die wählbaren Quellen. */
  quellen: { value: Quelle; label: string }[];
  /** Was unter der Überschrift steht, z. B. die Quelle in Worten. */
  untertitel: string;
}>();

const emit = defineEmits<{ entfernen: [] }>();

/** Der Fehler zu einem Teil des Felds. */
function feldFehler(teil: string): string | undefined {
  return props.fehler[`${props.praefix}.${props.nummer - 1}.${teil}`];
}

function quelleSetzen(wert: string | null): void {
  if (wert === "server" || wert === "client" || wert === "geheim") {
    props.feld.quelle = wert;
  }
}

/**
 * Link-Text und Link-Adresse setzen (Aufgabe 13). Solange der Link leer ist,
 * steht dort ein leerer Text; `eintrag_saeubern` macht daraus `null`.
 */
function linkSetzen(teil: "text" | "url", wert: string): void {
  const link = props.feld.link ?? { text: "", url: "" };
  props.feld.link = teil === "text" ? { ...link, text: wert } : { ...link, url: wert };
}

/** Die Art wechseln (Aufgabe 16); vorhandene Optionen bleiben erhalten. */
function artSetzen(wert: string | null): void {
  if (!wert) return;
  props.feld.art = feld_art_aus_name(wert, props.feld.art ?? "text");
}

/** Die Optionen einer Auswahl als Text setzen. */
function optionenSetzen(text: string): void {
  props.feld.art = { auswahl: { optionen: optionen_aus_text(text) } };
}
</script>

<template>
  <fieldset class="feldkarte">
    <legend class="feldkarte__titel">Feld {{ nummer }}</legend>
    <div class="feldkarte__kopf">
      <span class="blass">
        {{ untertitel }} · {{ feld_art_text(feld.art ?? "text") }}
      </span>
      <WpIconButton
        :icon="Trash2"
        label="Feld entfernen"
        size="sm"
        variant="ghost"
        @press="emit('entfernen')"
      />
    </div>

    <Feld beschriftung="Name" :fehler="feldFehler('name')">
      <template #default="{ id }">
        <WpInput
          :id="id"
          :model-value="feld.name"
          :state="feldFehler('name') ? 'invalid' : 'default'"
          placeholder="konto"
          @update:model-value="(wert) => (feld.name = wert)"
        />
      </template>
    </Feld>

    <Feld beschriftung="Beschreibung">
      <template #default="{ id }">
        <WpInput
          :id="id"
          :model-value="feld.beschreibung"
          @update:model-value="(wert) => (feld.beschreibung = wert)"
        />
      </template>
    </Feld>

    <div class="feldkarte__zeile">
      <Auswahl
        :model-value="feld.art ? feld_art_name(feld.art) : 'text'"
        :options="FELD_ARTEN"
        label="Art"
        @update:model-value="artSetzen"
      />
      <Auswahl
        :model-value="feld.quelle"
        :options="quellen"
        label="Quelle"
        @update:model-value="quelleSetzen"
      />
      <WpCheckbox
        :model-value="feld.pflicht"
        label="Pflicht"
        @update:model-value="(wert) => (feld.pflicht = wert)"
      />
    </div>

    <Feld
      beschriftung="Titel"
      hinweis="Leer heißt: der Feldname steht im Dialog."
    >
      <template #default="{ id }">
        <WpInput
          :id="id"
          :model-value="feld.titel ?? ''"
          placeholder="SAP-Benutzer"
          @update:model-value="
            (wert) => (feld.titel = wert.length > 0 ? wert : null)
          "
        />
      </template>
    </Feld>

    <Feld
      v-if="feld_art_name(feld.art ?? 'text') === 'auswahl'"
      beschriftung="Optionen"
      hinweis="Eine je Zeile: wert | Anzeige."
    >
      <template #default="{ id }">
        <WpTextarea
          :id="id"
          :rows="4"
          :model-value="optionen_als_text(feld_optionen(feld))"
          placeholder="rot | Rot"
          @update:model-value="optionenSetzen"
        />
      </template>
    </Feld>

    <Feld beschriftung="Standard" hinweis="Leer heißt: kein Standardwert.">
      <template #default="{ id }">
        <WpInput
          :id="id"
          :model-value="feld.standard ?? ''"
          @update:model-value="
            (wert) => (feld.standard = wert.length > 0 ? wert : null)
          "
        />
      </template>
    </Feld>

    <div class="feldkarte__zeile">
      <Feld beschriftung="Link-Text" hinweis="Leer heißt: kein Hilfe-Link.">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="feld.link?.text ?? ''"
            placeholder="Token in Outline erstellen"
            @update:model-value="(wert) => linkSetzen('text', wert)"
          />
        </template>
      </Feld>

      <Feld beschriftung="Link-Adresse" hinweis="Platzhalter ${feld} sind erlaubt.">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="feld.link?.url ?? ''"
            placeholder="${url}/settings/api-and-access"
            @update:model-value="(wert) => linkSetzen('url', wert)"
          />
        </template>
      </Feld>
    </div>
  </fieldset>
</template>

<style scoped>
.feldkarte {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  margin: 0;
  padding: var(--wp-space-3);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  background: var(--wp-color-bg-raised);
}

.feldkarte__titel {
  padding-inline: var(--wp-space-1);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
  font-weight: var(--wp-font-weight-medium);
}

.feldkarte__kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-2);
}

.feldkarte__zeile {
  display: flex;
  align-items: flex-end;
  gap: var(--wp-space-3);
  flex-wrap: wrap;
}

.feldkarte__zeile > :first-child {
  flex: 1 1 calc(var(--wp-space-8) * 3);
  min-width: 0;
}
</style>
