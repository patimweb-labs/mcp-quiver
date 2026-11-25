<script setup lang="ts">
// Benutzer (Aufgabe 21): wer darf herein, welche Rolle, welche Entra-Gruppen,
// wie viele Geräte, wann zuletzt geprüft – und die Handlungen dazu, jede mit
// einer Rückfrage in verständlichem Text.
//
// Der Root ist der Notzugang (ADR 0002): Er wird gekennzeichnet und ist nicht
// änderbar. Rollen stehen als Daten in `logik/rollen.ts`; diese Ansicht zählt
// keine Rollen auf (Aufgabe 24).
import { computed, onMounted, ref, shallowRef } from "vue";
import { Ellipsis, RefreshCw, UserRound } from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpInput,
  WpMenu,
} from "@weppelmann/components";
import type { ListOption, MenuItem } from "@weppelmann/components";

import {
  abmelden as sitzungBeenden,
  anmeldearten,
  fehlertext,
  ich as wer_bin_ich,
  type Client,
} from "../api/client";
import type { BenutzerSicht, Gruppe, Rolle } from "../api/typen";
import Auswahl from "../bausteine/Auswahl.vue";
import Karte from "../bausteine/Karte.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import {
  type Benutzerfilter,
  type Lagefilter,
  type Rollenfilter,
  aktionen,
  benutzer_name,
  filtern,
  geraete_text,
  gruppen_zeilen,
  loesch_frage,
  pruef_frage,
  pruefstand,
  rollen_aendern,
  rollen_frage,
  rollenknopf,
  sperre_aendern,
  sperre_frage,
  zielrolle,
} from "../logik/benutzer";
import { zeitpunkt } from "../logik/format";
import { ROLLEN, rolle_eintrag, rollen_text } from "../logik/rollen";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";
import GruppenVonBenutzer from "./GruppenVonBenutzer.vue";

const props = defineProps<{ client: Client }>();

interface Daten {
  liste: BenutzerSicht[];
  /** Alle Gruppen: für „Gruppen hinzufügen“ (Aufgabe 33). */
  gruppen: Gruppe[];
  entra: boolean;
  /** Die Objekt-Id der eigenen Sitzung; für „das bist du“. */
  selbst: string | null;
}

type Handlung = "rolle" | "sperre" | "pruefen" | "loeschen";

interface Auftrag {
  handlung: Handlung;
  benutzer: BenutzerSicht;
}

const zustand = shallowRef<Zustand<Daten>>(laedt());
const suche = ref("");
const rollenfilter = ref<Rollenfilter>("alle");
const lagefilter = ref<Lagefilter>("alle");

const auftrag = shallowRef<Auftrag | null>(null);
const laeuft = ref(false);
const auftragsfehler = ref("");
const meldung = ref("");

const daten = computed(() =>
  zustand.value.art === "fertig" ? zustand.value.daten : null,
);

const filter = computed<Benutzerfilter>(() => ({
  suche: suche.value,
  rolle: rollenfilter.value,
  lage: lagefilter.value,
}));

const gefiltert = computed(() =>
  daten.value ? filtern(daten.value.liste, filter.value) : [],
);

const rollenOptionen: ListOption[] = [
  { value: "alle", label: "Alle Rollen" },
  ...ROLLEN.map((eintrag) => ({
    value: eintrag.wert,
    label: eintrag.titel,
    hint: eintrag.unter,
  })),
];

const lageOptionen: ListOption[] = [
  { value: "alle", label: "Jeder Stand" },
  { value: "aktiv", label: "Aktiv" },
  { value: "gesperrt", label: "Gesperrt" },
  { value: "neu_anmelden", label: "Neu anmelden nötig" },
];

function setzeRolle(wert: string | null): void {
  const bekannt = ROLLEN.some((eintrag) => eintrag.wert === wert);
  rollenfilter.value = bekannt ? (wert as Rolle) : "alle";
}

function setzeLage(wert: string | null): void {
  const erlaubt: Lagefilter[] = ["alle", "aktiv", "gesperrt", "neu_anmelden"];
  lagefilter.value = erlaubt.includes(wert as Lagefilter)
    ? (wert as Lagefilter)
    : "alle";
}

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    const [liste, gruppen, arten, eigen] = await Promise.all([
      props.client.benutzer(),
      props.client.gruppen(),
      anmeldearten(),
      wer_bin_ich(),
    ]);
    zustand.value = fertig({
      liste,
      gruppen,
      entra: arten.entra,
      selbst: eigen?.benutzer.oid ?? null,
    });
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

function istSelbst(benutzer: BenutzerSicht): boolean {
  return daten.value?.selbst === benutzer.oid;
}

function gruppen(benutzer: BenutzerSicht) {
  return gruppen_zeilen(benutzer);
}

/** Das Menü einer Zeile; leer beim Root, dann gibt es keinen Knopf. */
function menue(benutzer: BenutzerSicht): MenuItem[] {
  const erlaubt = aktionen(benutzer, daten.value?.entra ?? false);
  const punkte: MenuItem[] = [];
  if (erlaubt.rolle) punkte.push({ id: "rolle", label: rollenknopf(benutzer) });
  if (erlaubt.pruefen) punkte.push({ id: "pruefen", label: "Jetzt prüfen" });
  if (erlaubt.gruppen) punkte.push({ id: "gruppen", label: "Gruppen" });
  if (erlaubt.sperre) {
    punkte.push({
      id: "sperre",
      label: benutzer.gesperrt ? "Entsperren" : "Sperren",
    });
  }
  if (erlaubt.loeschen) {
    punkte.push({
      id: "loeschen",
      label: "Löschen",
      danger: true,
      separatorBefore: true,
    });
  }
  return punkte;
}

const gruppenDialog = ref(false);
const gruppenBenutzer = shallowRef<BenutzerSicht | null>(null);

/** „Gruppen“ ist kein Bestätigungsdialog, sondern eine eigene Ansicht. */
function waehle(id: string, benutzer: BenutzerSicht): void {
  if (id === "gruppen") {
    gruppenBenutzer.value = benutzer;
    gruppenDialog.value = true;
    return;
  }
  oeffne(id as Handlung, benutzer);
}

function oeffne(handlung: Handlung, benutzer: BenutzerSicht): void {
  auftragsfehler.value = "";
  meldung.value = "";
  auftrag.value = { handlung, benutzer };
}

function schliesse(): void {
  auftrag.value = null;
  auftragsfehler.value = "";
}

const auftragsTitel = computed(() => {
  const a = auftrag.value;
  if (!a) return "";
  switch (a.handlung) {
    case "rolle":
      return "Rolle ändern";
    case "sperre":
      return a.benutzer.gesperrt ? "Benutzer entsperren" : "Benutzer sperren";
    case "pruefen":
      return "Jetzt prüfen";
    case "loeschen":
      return "Benutzer löschen";
  }
});

const auftragsFrage = computed(() => {
  const a = auftrag.value;
  if (!a) return "";
  const selbst = istSelbst(a.benutzer);
  switch (a.handlung) {
    case "rolle":
      return rollen_frage(a.benutzer, selbst);
    case "sperre":
      return sperre_frage(a.benutzer, selbst);
    case "pruefen":
      return pruef_frage(a.benutzer);
    case "loeschen":
      return loesch_frage(a.benutzer, selbst);
  }
});

const auftragsKnopf = computed(() => {
  const a = auftrag.value;
  if (!a) return "";
  switch (a.handlung) {
    case "rolle":
      return zielrolle(a.benutzer) === "admin"
        ? "Zum Admin"
        : "Zum Mitarbeiter";
    case "sperre":
      return a.benutzer.gesperrt ? "Entsperren" : "Sperren";
    case "pruefen":
      return "Jetzt prüfen";
    case "loeschen":
      return "Löschen";
  }
});

const auftragsTon = computed<"info" | "warn" | "error">(() =>
  auftrag.value?.handlung === "loeschen" ? "error" : "warn",
);

/** Nach einer Handlung, die die eigenen Rechte nimmt: zurück zur Anmeldung. */
async function zurueckZurAnmeldung(): Promise<void> {
  await sitzungBeenden().catch(() => {
    // Ohne Server ist die Sitzung auch so verloren; der Neuladen-Weg bleibt.
  });
  window.location.assign("/");
}

async function ausfuehren(): Promise<void> {
  const a = auftrag.value;
  if (!a) return;
  laeuft.value = true;
  auftragsfehler.value = "";
  const selbst = istSelbst(a.benutzer);
  const name = benutzer_name(a.benutzer);
  try {
    switch (a.handlung) {
      case "rolle": {
        const rumpf = rollen_aendern(a.benutzer, selbst);
        if (!rumpf) return;
        await props.client.benutzerSetzen(a.benutzer.oid, rumpf);
        if (selbst) {
          await zurueckZurAnmeldung();
          return;
        }
        meldung.value = `${name} ist jetzt ${rollen_text(
          rumpf.rolle ?? a.benutzer.rolle,
        )}.`;
        break;
      }
      case "sperre": {
        await props.client.benutzerSetzen(
          a.benutzer.oid,
          sperre_aendern(a.benutzer, selbst),
        );
        if (selbst) {
          await zurueckZurAnmeldung();
          return;
        }
        meldung.value = a.benutzer.gesperrt
          ? `${name} ist entsperrt.`
          : `${name} ist gesperrt; alle Sitzungen sind beendet.`;
        break;
      }
      case "pruefen": {
        await props.client.benutzerPruefen(a.benutzer.oid);
        meldung.value = `Die Entra-Gruppen von ${name} sind nachgeprüft.`;
        break;
      }
      case "loeschen": {
        await props.client.benutzerLoeschen(a.benutzer.oid);
        meldung.value = `${name} ist gelöscht.`;
        break;
      }
    }
    auftrag.value = null;
    await laden();
  } catch (fehlerWert) {
    auftragsfehler.value = fehlertext(fehlerWert);
  } finally {
    laeuft.value = false;
  }
}

function filterLeeren(): void {
  suche.value = "";
  rollenfilter.value = "alle";
  lagefilter.value = "alle";
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Benutzer"
      unter="Wer hereinkommt, welche Rolle gilt, welche Entra-Gruppen und Geräte."
    />

    <div class="werkzeuge">
      <div class="werkzeuge__suche">
        <WpInput
          v-model="suche"
          type="search"
          placeholder="Suchen (Name, E-Mail, Objekt-Id)"
          aria-label="Benutzer durchsuchen"
        />
      </div>
      <div class="werkzeuge__filter">
        <Auswahl
          :model-value="rollenfilter"
          :options="rollenOptionen"
          label="Rolle"
          @update:model-value="setzeRolle"
        />
        <Auswahl
          :model-value="lagefilter"
          :options="lageOptionen"
          label="Stand"
          @update:model-value="setzeLage"
        />
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </div>
    </div>

    <p v-if="meldung" class="meldung" role="status">
      <Status ton="gut" :text="meldung" />
    </p>

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Benutzer und Entra-Gruppen werden geholt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </Karte>
    </template>

    <template v-else-if="zustand.daten.liste.length === 0">
      <Leer
        titel="Noch keine Benutzer"
        :erklaerung="
          zustand.daten.entra
            ? 'Ein Benutzer erscheint hier, sobald er sich das erste Mal über Microsoft anmeldet.'
            : 'Ohne Entra gibt es nur den Root; Geräte melden sich mit einem Einmal-Code an.'
        "
      />
    </template>

    <template v-else-if="gefiltert.length === 0">
      <Leer
        titel="Keine Treffer"
        :erklaerung="
          suche.trim()
            ? `Nichts passt zu „${suche}“ und dem gewählten Filter.`
            : 'Kein Benutzer passt zum gewählten Filter.'
        "
      >
        <template #aktion>
          <WpButton label="Filter leeren" @press="filterLeeren" />
        </template>
      </Leer>
    </template>

    <Karte
      v-else
      titel="Benutzer"
      :unter="`${gefiltert.length} von ${zustand.daten.liste.length} Benutzern · neue kommen mit ihrer ersten Anmeldung dazu`"
    >
      <table class="tabelle tabelle--personen">
        <colgroup>
          <col class="spalte--person" />
          <col class="spalte--rolle" />
          <col />
          <col class="spalte--geraete" />
          <col class="spalte--stand" />
          <col class="spalte--menue" />
        </colgroup>
        <thead>
          <tr>
            <th scope="col">Benutzer</th>
            <th scope="col">Rolle</th>
            <th scope="col">Gruppen</th>
            <th scope="col">Geräte</th>
            <th scope="col">Stand</th>
            <th scope="col"><span class="nur-vorlesen">Aktionen</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="benutzer in gefiltert" :key="benutzer.oid">
            <td>
              <div class="person">
                <UserRound class="person__zeichen" aria-hidden="true" />
                <div class="person__text">
                  <span class="person__name">
                    <span class="kurz" :title="benutzer_name(benutzer)">{{
                      benutzer_name(benutzer)
                    }}</span>
                    <span v-if="istSelbst(benutzer)" class="marke-chip">du</span>
                    <span
                      v-if="benutzer.gesperrt"
                      class="marke-chip marke-chip--warnung"
                      >gesperrt</span
                    >
                  </span>
                  <span
                    v-if="benutzer.email"
                    class="kurz leise klein"
                    :title="benutzer.email"
                    >{{ benutzer.email }}</span
                  >
                </div>
              </div>
            </td>
            <td data-label="Rolle">
              <Status
                :ton="rolle_eintrag(benutzer.rolle).ton"
                :text="rollen_text(benutzer.rolle)"
              />
            </td>
            <td data-label="Gruppen">
              <ul v-if="gruppen(benutzer).length > 0" class="chips">
                <li
                  v-for="gruppe in gruppen(benutzer)"
                  :key="gruppe.id"
                  class="marke-chip kurz"
                  :title="gruppe.hinweis"
                >
                  {{ gruppe.titel }}
                </li>
              </ul>
              <span v-else class="blass">—</span>
            </td>
            <td data-label="Geräte">
              <span :title="geraete_text(benutzer.geraete)">{{
                benutzer.geraete
              }}</span>
            </td>
            <td data-label="Stand">
              <div
                class="stand"
                :title="pruefstand(benutzer, zustand.daten.entra).hinweis"
              >
                <Status
                  :ton="pruefstand(benutzer, zustand.daten.entra).ton"
                  :text="pruefstand(benutzer, zustand.daten.entra).text"
                />
                <span class="leise klein einzeilig">{{
                  zeitpunkt(benutzer.zuletzt)
                }}</span>
              </div>
            </td>
            <td class="zelle--menue">
              <WpMenu
                v-if="menue(benutzer).length > 0"
                :items="menue(benutzer)"
                :label="`Aktionen für ${benutzer_name(benutzer)}`"
                @select="(id) => waehle(id, benutzer)"
              >
                <Ellipsis class="menuezeichen" aria-hidden="true" />
              </WpMenu>
            </td>
          </tr>
        </tbody>
      </table>
    </Karte>

    <GruppenVonBenutzer
      v-model:offen="gruppenDialog"
      :benutzer="gruppenBenutzer"
      :alle-gruppen="daten?.gruppen ?? []"
      :client="client"
      :selbst="daten?.selbst ?? null"
      @geaendert="laden"
    />

    <WpDialog
      :open="auftrag !== null"
      :title="auftragsTitel"
      :description="auftragsFrage"
      :tone="auftragsTon"
      @update:open="(wert) => (!wert ? schliesse() : undefined)"
    >
      <p v-if="auftragsfehler" class="fehler" role="alert">
        {{ auftragsfehler }}
      </p>
      <template #actions>
        <WpButton label="Abbrechen" @press="schliesse" />
        <WpButton
          :label="auftragsKnopf"
          :variant="auftrag?.handlung === 'loeschen' ? 'danger' : 'primary'"
          :state="laeuft ? 'busy' : 'default'"
          @press="ausfuehren"
        />
      </template>
    </WpDialog>
  </div>
</template>

<style scoped>
.seite {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-4);
}

.werkzeuge {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.werkzeuge__suche {
  flex: 1 1 calc(var(--wp-space-8) * 4);
  min-width: 0;
}

.werkzeuge__filter {
  display: flex;
  align-items: flex-end;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.meldung {
  display: flex;
}

/* Feste Spalten: die Breite folgt dem Fenster, nicht dem längsten Namen. Was
   nicht passt, wird gekürzt und steht vollständig im Hinweis. */
.tabelle--personen {
  table-layout: fixed;
}

.tabelle--personen td {
  vertical-align: middle;
}

.spalte--person {
  width: 34%;
}

.spalte--rolle {
  width: calc(var(--wp-space-8) * 2);
}

.spalte--geraete {
  width: calc(var(--wp-space-8) * 1.25);
}

.spalte--stand {
  width: calc(var(--wp-space-8) * 2.25);
}

.spalte--menue {
  width: var(--wp-space-8);
}

.zelle--menue {
  text-align: end;
}

.person {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  min-width: 0;
}

.person__zeichen {
  flex: none;
  width: var(--wp-space-4);
  height: var(--wp-space-4);
  color: var(--wp-color-text-muted);
}

.person__text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.person__name {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  min-width: 0;
  font-weight: var(--wp-font-weight-medium);
}

.stand {
  display: flex;
  flex-direction: column;
}

.kurz {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.klein {
  font-size: var(--wp-font-size-sm);
}

.einzeilig {
  white-space: nowrap;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--wp-space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.chips .marke-chip {
  flex: 0 1 auto;
  max-width: 100%;
}

.menuezeichen {
  display: block;
  width: var(--wp-space-4);
  height: var(--wp-space-4);
  margin: var(--wp-space-1);
  color: var(--wp-color-text-muted);
}

/* Unter `breakpoint.formular` wird aus der Tabelle eine Liste: oben die Person
   mit dem Menü, darunter die übrigen Angaben (Aufgabe 18). */
:root[data-breite="eng"] .tabelle--personen colgroup,
:root[data-breite="eng"] .tabelle--personen thead {
  display: none;
}

:root[data-breite="eng"] .tabelle--personen,
:root[data-breite="eng"] .tabelle--personen tbody {
  display: block;
}

:root[data-breite="eng"] .tabelle--personen tr {
  position: relative;
  display: block;
  padding-block: var(--wp-space-2);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

:root[data-breite="eng"] .tabelle--personen tr:last-child {
  border-bottom: 0;
}

:root[data-breite="eng"] .tabelle--personen td {
  display: grid;
  grid-template-columns: calc(var(--wp-space-8) * 1.5) minmax(0, 1fr);
  gap: var(--wp-space-2);
  padding: var(--wp-space-1) 0;
  border: 0;
}

:root[data-breite="eng"] .tabelle--personen td:first-child {
  display: block;
  padding-inline-end: var(--wp-space-8);
}

:root[data-breite="eng"] .tabelle--personen td[data-label]::before {
  content: attr(data-label);
  color: var(--wp-color-text-faint);
  font-size: var(--wp-font-size-sm);
}

:root[data-breite="eng"] .tabelle--personen .zelle--menue {
  position: absolute;
  display: block;
  inset-block-start: var(--wp-space-2);
  inset-inline-end: 0;
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
