<script setup lang="ts">
// Der Abschnitt „Mitglieder“ einer Gruppe (Aufgabe 33): wer über Entra drin
// ist, wer von Hand dazugekommen ist, und beides von hier aus pflegen.
//
// Geändert wird sofort (je Zugehörigkeit eine Anfrage), nicht erst beim
// Speichern des Dialogs: die Mitgliedschaft gehört dem Benutzer, nicht dem
// Formular. Der Server schickt den neuen Stand zurück.
import { computed, ref, shallowRef, watch } from "vue";
import { Search, Trash2, UserRound } from "lucide-vue-next";
import { WpButton, WpDialog, WpIconButton } from "@weppelmann/components";
import type { ListOption } from "@weppelmann/components";

import { fehlertext, ich as wer_bin_ich, type Client } from "../api/client";
import type { BenutzerSicht, Gruppe, Gruppenmitglied } from "../api/typen";
import Mehrfachauswahl from "../bausteine/Mehrfachauswahl.vue";
import Status from "../bausteine/Status.vue";
import { zeitpunkt } from "../logik/format";
import {
  entra_hinweis,
  gesperrt_hinweis,
  herkuenfte,
  kandidaten,
  letzte_gruppe,
  mitglied_name,
  mitglied_zeile,
  mitglieder_text,
  von_hand_anzahl,
} from "../logik/mitglieder";
import { type Zustand, daten_von, fehler, fertig, laedt } from "../logik/seite";

const props = defineProps<{
  gruppe: Gruppe | null;
  client: Client;
}>();

interface Daten {
  mitglieder: Gruppenmitglied[];
  benutzer: BenutzerSicht[];
  /** Die eigene Objekt-Id: beim Aussperren wird gewarnt. */
  selbst: string | null;
}

const zustand = shallowRef<Zustand<Daten>>(laedt());
const meldung = ref("");
const aktionsfehler = ref("");
const laeuft = ref(false);
const gewaehlt = ref<string[]>([]);
const zu_entfernen = shallowRef<Gruppenmitglied | null>(null);

const daten = computed(() => daten_von(zustand.value));

const kandidatenOptionen = computed<ListOption[]>(() => {
  const aktuell = daten.value;
  if (!aktuell) return [];
  return kandidaten(aktuell.benutzer, aktuell.mitglieder, "").map((eintrag) => ({
    value: eintrag.oid,
    label: mitglied_name(eintrag),
    hint: [eintrag.email ?? "", eintrag.gesperrt ? "gesperrt" : ""]
      .filter(Boolean)
      .join(" · "),
  }));
});

const entfernen_selbst = computed(
  () => zu_entfernen.value !== null && zu_entfernen.value.oid === daten.value?.selbst,
);

/**
 * Nimmt das Entfernen der angemeldeten Person die letzte Gruppe? Dann sagt der
 * Dialog das deutlich, und der Server bekommt die Bestätigung.
 */
const letzte_eigene = computed(() => {
  const eigen = daten.value?.selbst;
  if (!eigen) return false;
  const person = daten.value?.benutzer.find((eintrag) => eintrag.oid === eigen);
  if (!person || !props.gruppe) return false;
  return letzte_gruppe(person.gruppen, props.gruppe.id);
});

const warnung = computed(() =>
  entfernen_selbst.value && letzte_eigene.value
    ? "Das ist deine letzte Gruppe. Danach gilt „kein Zugriff“: deine Geräte " +
      "bekommen keine Tools mehr, bis dir jemand wieder eine Gruppe gibt."
    : "",
);

/** Was im Bestätigungsdialog steht – ein Satz, je nach Lage. */
const entfernen_text = computed(() => {
  const ziel = zu_entfernen.value;
  if (!ziel) return "";
  const name = mitglied_name(ziel);
  if (!entfernen_selbst.value) {
    return `${name} wird von Hand aus dieser Gruppe entfernt. Was über Entra kommt, bleibt.`;
  }
  if (warnung.value) return `${name} wird von Hand entfernt – das bist du selbst. ${warnung.value}`;
  return `${name} wird von Hand entfernt – das bist du selbst. Was über Entra kommt, bleibt.`;
});

async function laden(): Promise<void> {
  const gruppe = props.gruppe;
  if (!gruppe) return;
  zustand.value = laedt();
  try {
    const [mitglieder, benutzer, eigen] = await Promise.all([
      props.client.gruppeMitglieder(gruppe.id),
      props.client.benutzer(),
      wer_bin_ich(),
    ]);
    zustand.value = fertig({
      mitglieder,
      benutzer,
      selbst: eigen?.benutzer.oid ?? null,
    });
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

async function hinzufuegen(): Promise<void> {
  const gruppe = props.gruppe;
  if (!gruppe || gewaehlt.value.length === 0) return;
  laeuft.value = true;
  aktionsfehler.value = "";
  meldung.value = "";
  const anzahl = gewaehlt.value.length;
  try {
    for (const oid of [...gewaehlt.value]) {
      await props.client.gruppeMitgliedSetzen(gruppe.id, oid, true);
    }
    gewaehlt.value = [];
    meldung.value = `${anzahl} ${anzahl === 1 ? "Mitglied" : "Mitglieder"} hinzugefügt.`;
    await laden();
  } catch (fehlerWert) {
    aktionsfehler.value = fehlertext(fehlerWert);
  } finally {
    laeuft.value = false;
  }
}

async function entfernen(bestaetigt: boolean): Promise<void> {
  const gruppe = props.gruppe;
  const ziel = zu_entfernen.value;
  if (!gruppe || !ziel) return;
  laeuft.value = true;
  aktionsfehler.value = "";
  meldung.value = "";
  try {
    await props.client.gruppeMitgliedSetzen(gruppe.id, ziel.oid, false, bestaetigt);
    zu_entfernen.value = null;
    meldung.value = `${mitglied_name(ziel)} entfernt.`;
    await laden();
  } catch (fehlerWert) {
    aktionsfehler.value = fehlertext(fehlerWert);
  } finally {
    laeuft.value = false;
  }
}

watch(
  () => props.gruppe?.id,
  () => {
    gewaehlt.value = [];
    meldung.value = "";
    aktionsfehler.value = "";
    zu_entfernen.value = null;
    if (props.gruppe) void laden();
  },
  { immediate: true },
);
</script>

<template>
  <section v-if="gruppe" class="mitglieder" aria-labelledby="mitglieder-titel">
    <div class="mitglieder__kopf">
      <h3 id="mitglieder-titel" class="mitglieder__titel">Mitglieder</h3>
      <span v-if="daten" class="mitglieder__zaehler">
        {{ mitglieder_text(daten.mitglieder.length) }}
        <template v-if="von_hand_anzahl(daten.mitglieder) > 0">
          · {{ von_hand_anzahl(daten.mitglieder) }} von Hand
        </template>
      </span>
    </div>

    <p v-if="meldung" class="mitglieder__meldung" role="status">
      <Status ton="gut" :text="meldung" />
    </p>
    <p v-if="aktionsfehler" class="mitglieder__fehler" role="alert">{{ aktionsfehler }}</p>

    <p v-if="zustand.art === 'laedt'" class="mitglieder__leer">Lädt …</p>
    <p v-else-if="zustand.art === 'fehler'" class="mitglieder__fehler" role="alert">
      {{ zustand.text }}
    </p>

    <template v-else>
      <ul v-if="daten && daten.mitglieder.length > 0" class="mitglieder__liste">
        <li
          v-for="eintrag in daten.mitglieder"
          :key="eintrag.oid"
          class="mitglieder__zeile"
        >
          <UserRound class="mitglieder__zeichen" aria-hidden="true" />
          <div class="mitglieder__haupt">
            <div class="mitglieder__namen">
              <span class="mitglieder__name">{{ mitglied_name(eintrag) }}</span>
              <span
                v-for="herkunft in herkuenfte(eintrag)"
                :key="herkunft.text"
                class="marke-chip"
                :title="herkunft.hinweis"
              >
                {{ herkunft.text }}
              </span>
              <span v-if="eintrag.gesperrt" class="marke-chip marke-chip--warnung">
                gesperrt
              </span>
            </div>
            <span class="mitglieder__klein">{{ mitglied_zeile(eintrag) }}</span>
            <span v-if="eintrag.von_hand && eintrag.seit" class="mitglieder__klein">
              von Hand seit {{ zeitpunkt(eintrag.seit) }}
              <template v-if="eintrag.von"> · von {{ eintrag.von }}</template>
            </span>
            <span v-if="gesperrt_hinweis(eintrag)" class="mitglieder__klein">
              {{ gesperrt_hinweis(eintrag) }}
            </span>
            <span v-if="entra_hinweis(eintrag)" class="mitglieder__klein">
              {{ entra_hinweis(eintrag) }}
            </span>
          </div>
          <div class="mitglieder__aktionen">
            <WpIconButton
              v-if="eintrag.von_hand"
              :icon="Trash2"
              label="Von Hand entfernen"
              variant="ghost"
              @press="zu_entfernen = eintrag"
            />
          </div>
        </li>
      </ul>
      <p v-else class="mitglieder__leer">
        Noch niemand in dieser Gruppe. Wer über Entra dazukommt, erscheint hier von
        selbst.
      </p>

      <div class="mitglieder__hinzufuegen">
        <Mehrfachauswahl
          v-model="gewaehlt"
          :options="kandidatenOptionen"
          :label="`Benutzer hinzufügen (${kandidatenOptionen.length} zur Auswahl)`"
          placeholder="Benutzer suchen und auswählen …"
          suchbar
        />
        <WpButton
          label="Hinzufügen"
          :state="laeuft ? 'busy' : 'default'"
          @press="hinzufuegen"
        />
      </div>
      <p class="mitglieder__klein">
        <Search class="mitglieder__suchzeichen" aria-hidden="true" />
        Tippen filtert die Liste; der Fokus bleibt im Feld. Gezeigt werden nur
        Benutzer, die der Server kennt.
      </p>
    </template>

    <WpDialog
      :open="zu_entfernen !== null"
      title="Aus der Gruppe entfernen"
      :description="entfernen_text"
      :tone="warnung ? 'warn' : 'info'"
      @update:open="(wert) => (!wert ? (zu_entfernen = null) : undefined)"
    >
      <template #actions>
        <WpButton label="Abbrechen" @press="zu_entfernen = null" />
        <WpButton
          label="Entfernen"
          variant="danger"
          :state="laeuft ? 'busy' : 'default'"
          @press="entfernen(entfernen_selbst && letzte_eigene)"
        />
      </template>
    </WpDialog>
  </section>
</template>

<style scoped>
.mitglieder {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
  margin-top: var(--wp-space-3);
  padding-top: var(--wp-space-3);
  border-top: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.mitglieder__kopf {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.mitglieder__titel {
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-semibold);
}

.mitglieder__zaehler {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.mitglieder__liste {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
  max-height: calc(var(--wp-space-8) * 6);
  overflow-y: auto;
}

.mitglieder__zeile {
  display: flex;
  align-items: flex-start;
  gap: var(--wp-space-2);
  padding: var(--wp-space-2) 0;
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.mitglieder__zeile:last-child {
  border-bottom: 0;
}

.mitglieder__zeichen {
  flex: none;
  width: var(--wp-space-4);
  height: var(--wp-space-4);
  margin-top: 2px;
  color: var(--wp-color-text-muted);
}

.mitglieder__haupt {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.mitglieder__namen {
  display: flex;
  align-items: center;
  gap: var(--wp-space-1);
  flex-wrap: wrap;
}

.mitglieder__name {
  font-weight: var(--wp-font-weight-medium);
}

.mitglieder__klein {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.mitglieder__aktionen {
  display: flex;
  flex: none;
  gap: var(--wp-space-1);
}

.mitglieder__hinzufuegen {
  display: flex;
  align-items: flex-end;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.mitglieder__hinzufuegen > :first-child {
  flex: 1 1 calc(var(--wp-space-8) * 4);
  min-width: 0;
}

.mitglieder__suchzeichen {
  width: var(--wp-font-size-sm);
  height: var(--wp-font-size-sm);
}

.mitglieder__leer {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.mitglieder__meldung {
  display: flex;
}

.mitglieder__fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
