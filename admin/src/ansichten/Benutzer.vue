<script setup lang="ts">
// Benutzer (Aufgabe 21): wer darf herein, welche Rolle, welche Entra-Gruppen,
// wie viele Geräte, wann zuletzt geprüft – und die Handlungen dazu, jede mit
// einer Rückfrage in verständlichem Text.
//
// Der Root ist der Notzugang (ADR 0002): Er wird gekennzeichnet und ist nicht
// änderbar. Rollen stehen als Daten in `logik/rollen.ts`; diese Ansicht zählt
// keine Rollen auf (Aufgabe 24).
import { computed, onMounted, ref, shallowRef } from "vue";
import {
  Lock,
  LockOpen,
  RefreshCw,
  ShieldCheck,
  Trash2,
  UserRound,
  UserRoundMinus,
} from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpIconButton,
  WpInput,
  WpSelect,
} from "@weppelmann/components";
import type { ListOption } from "@weppelmann/components";

import {
  abmelden as sitzungBeenden,
  anmeldearten,
  fehlertext,
  ich as wer_bin_ich,
  type Client,
} from "../api/client";
import type { BenutzerSicht, EntraGruppe, Rolle } from "../api/typen";
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
  entra_gruppen,
  filtern,
  geraete_text,
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

const props = defineProps<{ client: Client }>();

interface Daten {
  liste: BenutzerSicht[];
  gesehen: EntraGruppe[];
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
    const [liste, gesehen, arten, eigen] = await Promise.all([
      props.client.benutzer(),
      props.client.entraGruppen(),
      anmeldearten(),
      wer_bin_ich(),
    ]);
    zustand.value = fertig({
      liste,
      gesehen,
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
  return entra_gruppen(benutzer, daten.value?.gesehen ?? []);
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
        <WpSelect
          :model-value="rollenfilter"
          :options="rollenOptionen"
          label="Rolle"
          @update:model-value="setzeRolle"
        />
        <WpSelect
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
      <div class="tabelle__rahmen">
        <table class="tabelle tabelle--personen">
          <thead>
            <tr>
              <th scope="col">Name</th>
              <th scope="col">E-Mail</th>
              <th scope="col">Rolle</th>
              <th scope="col">Entra-Gruppen</th>
              <th scope="col">Geräte</th>
              <th scope="col">Prüfung</th>
              <th scope="col">Zuletzt</th>
              <th scope="col">Aktionen</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="benutzer in gefiltert" :key="benutzer.oid">
              <td data-label="Name">
                <div class="name">
                  <UserRound class="name__zeichen" aria-hidden="true" />
                  <span>{{ benutzer_name(benutzer) }}</span>
                  <span v-if="istSelbst(benutzer)" class="marke-chip">du</span>
                  <span
                    v-if="benutzer.gesperrt"
                    class="marke-chip marke-chip--warnung"
                  >
                    gesperrt
                  </span>
                  <span
                    v-if="rolle_eintrag(benutzer.rolle).fest"
                    class="marke-chip"
                    :title="rolle_eintrag(benutzer.rolle).unter"
                  >
                    Notzugang
                  </span>
                </div>
              </td>
              <td data-label="E-Mail">
                <span v-if="benutzer.email">{{ benutzer.email }}</span>
                <span v-else class="blass">—</span>
              </td>
              <td data-label="Rolle">
                <Status
                  :ton="rolle_eintrag(benutzer.rolle).ton"
                  :text="rollen_text(benutzer.rolle)"
                />
              </td>
              <td data-label="Entra-Gruppen">
                <ul v-if="gruppen(benutzer).length > 0" class="chips">
                  <li
                    v-for="gruppe in gruppen(benutzer)"
                    :key="gruppe.id"
                    class="marke-chip"
                    :title="gruppe.id"
                  >
                    {{ gruppe.name }}
                  </li>
                </ul>
                <span v-else class="blass">—</span>
              </td>
              <td data-label="Geräte">{{ geraete_text(benutzer.geraete) }}</td>
              <td data-label="Prüfung">
                <span :title="pruefstand(benutzer, zustand.daten.entra).hinweis">
                  <Status
                    :ton="pruefstand(benutzer, zustand.daten.entra).ton"
                    :text="pruefstand(benutzer, zustand.daten.entra).text"
                  />
                </span>
              </td>
              <td data-label="Zuletzt">{{ zeitpunkt(benutzer.zuletzt) }}</td>
              <td data-label="Aktionen">
                <div class="aktionen">
                  <template v-if="aktionen(benutzer, zustand.daten.entra).rolle">
                    <WpButton
                      :label="rollenknopf(benutzer)"
                      :icon="
                        zielrolle(benutzer) === 'admin'
                          ? ShieldCheck
                          : UserRoundMinus
                      "
                      size="sm"
                      @press="oeffne('rolle', benutzer)"
                    />
                  </template>
                  <template v-if="aktionen(benutzer, zustand.daten.entra).sperre">
                    <WpIconButton
                      :icon="benutzer.gesperrt ? LockOpen : Lock"
                      :label="benutzer.gesperrt ? 'Entsperren' : 'Sperren'"
                      size="sm"
                      @press="oeffne('sperre', benutzer)"
                    />
                  </template>
                  <template v-if="aktionen(benutzer, zustand.daten.entra).pruefen">
                    <WpIconButton
                      :icon="RefreshCw"
                      label="Jetzt prüfen"
                      size="sm"
                      @press="oeffne('pruefen', benutzer)"
                    />
                  </template>
                  <template v-if="aktionen(benutzer, zustand.daten.entra).loeschen">
                    <WpIconButton
                      :icon="Trash2"
                      label="Löschen"
                      size="sm"
                      variant="ghost"
                      @press="oeffne('loeschen', benutzer)"
                    />
                  </template>
                  <span
                    v-if="rolle_eintrag(benutzer.rolle).fest"
                    class="blass"
                  >
                    nicht änderbar
                  </span>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </Karte>

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

.tabelle__rahmen {
  overflow-x: auto;
}

/* Unter `breakpoint.formular` wird aus der Tabelle eine Liste: jede Zeile
   zeigt ihre Felder untereinander, mit dem Spaltennamen davor. Kein Inhalt
   verschwindet, nur die Anordnung ändert sich (Aufgabe 18). */
:root[data-breite="eng"] .tabelle--personen thead {
  display: none;
}

:root[data-breite="eng"] .tabelle--personen,
:root[data-breite="eng"] .tabelle--personen tbody,
:root[data-breite="eng"] .tabelle--personen tr,
:root[data-breite="eng"] .tabelle--personen td {
  display: block;
  width: auto;
}

:root[data-breite="eng"] .tabelle--personen tr {
  padding-block: var(--wp-space-2);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

:root[data-breite="eng"] .tabelle--personen tr:last-child {
  border-bottom: 0;
}

:root[data-breite="eng"] .tabelle--personen td {
  display: grid;
  grid-template-columns: calc(var(--wp-space-8) * 1.75) minmax(0, 1fr);
  gap: var(--wp-space-2);
  padding: var(--wp-space-1) var(--wp-space-2);
  border: 0;
}

:root[data-breite="eng"] .tabelle--personen td::before {
  content: attr(data-label);
  color: var(--wp-color-text-faint);
  font-size: var(--wp-font-size-sm);
}

.name {
  display: flex;
  align-items: center;
  gap: var(--wp-space-1);
  flex-wrap: wrap;
}

.name__zeichen {
  flex: none;
  width: var(--wp-font-size-md);
  height: var(--wp-font-size-md);
  color: var(--wp-color-text-muted);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--wp-space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.marke-chip {
  padding: 0 var(--wp-space-2);
  border: var(--wp-size-border) solid var(--wp-color-border-subtle);
  background: var(--wp-color-bg-raised);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-xs);
  line-height: var(--wp-size-control-sm);
}

.marke-chip--warnung {
  border-color: var(--wp-color-status-warn);
  color: var(--wp-color-status-warn);
}

.aktionen {
  display: flex;
  gap: var(--wp-space-1);
  flex-wrap: wrap;
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
