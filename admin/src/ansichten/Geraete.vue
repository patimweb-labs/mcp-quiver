<script setup lang="ts">
// Geräte: Tabelle mit aufklappbarem Stand je Gerät, Gruppen ändern, sperren,
// entfernen und ein neues Gerät über einen Einmal-Code einbinden.
import { computed, onMounted, ref, shallowRef } from "vue";
import {
  KeyRound,
  Lock,
  LockOpen,
  RefreshCw,
  Trash2,
  Users,
} from "lucide-vue-next";
import { WpButton, WpDialog } from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { fehlertext, type Client } from "../api/client";
import type { Eintrag, GeraetStand, Gruppe } from "../api/typen";
import Karte from "../bausteine/Karte.vue";
import Klappknopf from "../bausteine/Klappknopf.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import {
  geraet_name,
  gruppen_namen,
  revision_kurz,
  stand_text,
  stand_ton,
  wird_aktualisiert,
  zustand_eintraege,
  zustand_text,
  zustand_ton,
} from "../logik/geraet";
import { zeitpunkt } from "../logik/format";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";
import EinbindenDialog from "./EinbindenDialog.vue";
import GeraetDialog from "./GeraetDialog.vue";

const props = defineProps<{ client: Client }>();

interface Daten {
  geraete: GeraetStand[];
  gruppen: Gruppe[];
  titel: Map<string, string>;
}

const zustand = shallowRef<Zustand<Daten>>(laedt());
const offen = ref<string | null>(null);
const meldung = ref("");
const aktionsfehler = ref("");

const einbindenOffen = ref(false);
const geraetDialogOffen = ref(false);
const bearbeitet = shallowRef<GeraetStand | null>(null);
const sperrt = ref<string | null>(null);

const zuLoeschen = shallowRef<GeraetStand | null>(null);
const loeschfehler = ref("");
const loescht = ref(false);

const gruppenListe = computed(() =>
  zustand.value.art === "fertig" ? zustand.value.daten.gruppen : [],
);

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    const [geraete, gruppen, eintraege] = await Promise.all([
      props.client.geraete(),
      props.client.gruppen(),
      props.client.eintraege(),
    ]);
    zustand.value = fertig({
      geraete: [...geraete].sort((a, b) =>
        geraet_name(a).localeCompare(geraet_name(b), "de", {
          sensitivity: "base",
        }),
      ),
      gruppen,
      titel: titel_map(eintraege),
    });
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

function titel_map(eintraege: Eintrag[]): Map<string, string> {
  return new Map(eintraege.map((eintrag) => [eintrag.id, eintrag.titel]));
}

function umschalten(id: string): void {
  offen.value = offen.value === id ? null : id;
}

function gruppen_anzeige(stand: GeraetStand): string {
  if (zustand.value.art !== "fertig") return "—";
  const namen = gruppen_namen(
    stand.geraet.gruppen,
    zustand.value.daten.gruppen,
  );
  return namen.length > 0 ? namen.join(", ") : "—";
}

function titel(id: string): string {
  if (zustand.value.art !== "fertig") return id;
  return zustand.value.daten.titel.get(id) ?? id;
}

function revisionen(stand: GeraetStand): string {
  const gemeldet = stand.meldung
    ? revision_kurz(stand.meldung.revision)
    : "—";
  return `Soll ${revision_kurz(stand.soll_revision)} · gemeldet ${gemeldet}`;
}

function bearbeiten(stand: GeraetStand): void {
  aktionsfehler.value = "";
  bearbeitet.value = tief_kopie(stand);
  geraetDialogOffen.value = true;
}

async function gespeichert(stand: GeraetStand): Promise<void> {
  meldung.value = `Gerät ${stand.geraet.name || stand.geraet.id} gespeichert.`;
  await laden();
}

async function sperren(stand: GeraetStand): Promise<void> {
  sperrt.value = stand.geraet.id;
  aktionsfehler.value = "";
  try {
    await props.client.geraetSetzen(stand.geraet.id, {
      name: stand.geraet.name,
      gruppen: stand.geraet.gruppen,
      gesperrt: !stand.geraet.gesperrt,
    });
    meldung.value = stand.geraet.gesperrt
      ? `Gerät ${stand.geraet.id} entsperrt.`
      : `Gerät ${stand.geraet.id} gesperrt.`;
    await laden();
  } catch (fehlerWert) {
    aktionsfehler.value = fehlertext(fehlerWert);
  } finally {
    sperrt.value = null;
  }
}

async function entfernen(): Promise<void> {
  const ziel = zuLoeschen.value;
  if (!ziel) return;
  loescht.value = true;
  loeschfehler.value = "";
  try {
    await props.client.geraetLoeschen(ziel.geraet.id);
    zuLoeschen.value = null;
    offen.value = null;
    meldung.value = `Gerät ${ziel.geraet.id} entfernt.`;
    await laden();
  } catch (fehlerWert) {
    loeschfehler.value = fehlertext(fehlerWert);
  } finally {
    loescht.value = false;
  }
}

function schliesseLoeschen(): void {
  zuLoeschen.value = null;
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Geräte"
      unter="Stand je Gerät, Gruppen und Zugang über Einmal-Codes."
    />

    <div class="werkzeuge">
      <WpButton
        label="Gerät einbinden"
        variant="primary"
        :icon="KeyRound"
        @press="einbindenOffen = true"
      />
      <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
    </div>

    <p v-if="meldung" class="meldung" role="status">
      <Status ton="gut" :text="meldung" />
    </p>
    <p v-if="aktionsfehler" class="fehler" role="alert">{{ aktionsfehler }}</p>

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Geräte, Gruppen und Einträge werden geholt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </Karte>
    </template>

    <template v-else-if="zustand.daten.geraete.length === 0">
      <Leer
        titel="Noch keine Geräte"
        erklaerung="Ein Gerät bindet sich mit einem Einmal-Code an den Server."
      >
        <template #aktion>
          <WpButton
            label="Gerät einbinden"
            variant="primary"
            :icon="KeyRound"
            @press="einbindenOffen = true"
          />
        </template>
      </Leer>
    </template>

    <Karte v-else titel="Geräte" unter="Zeile öffnen zeigt den Zustand je Eintrag.">
      <div class="tabelle__rahmen">
        <table class="tabelle">
          <thead>
            <tr>
              <th scope="col">Name</th>
              <th scope="col">Gruppen</th>
              <th scope="col">Verbunden</th>
              <th scope="col">Zuletzt gesehen</th>
              <th scope="col">Stand</th>
            </tr>
          </thead>
          <tbody>
            <template
              v-for="(stand, i) in zustand.daten.geraete"
              :key="stand.geraet.id"
            >
              <tr>
                <td>
                  <Klappknopf
                    :offen="offen === stand.geraet.id"
                    :steuert="`geraet-detail-${i}`"
                    @umschalten="umschalten(stand.geraet.id)"
                  >
                    {{ geraet_name(stand) }}
                  </Klappknopf>
                  <p class="mono blass">{{ stand.geraet.id }}</p>
                </td>
                <td>{{ gruppen_anzeige(stand) }}</td>
                <td>
                  <Status
                    :ton="stand.verbunden ? 'gut' : 'leise'"
                    :text="stand.verbunden ? 'verbunden' : 'getrennt'"
                  />
                </td>
                <td>{{ zeitpunkt(stand.zuletzt_gesehen) }}</td>
                <td>
                  <span :title="revisionen(stand)">
                    <Status :ton="stand_ton(stand)" :text="stand_text(stand)" />
                  </span>
                </td>
              </tr>

              <tr v-if="offen === stand.geraet.id" :id="`geraet-detail-${i}`">
                <td colspan="5">
                  <div class="detail">
                    <div class="detail__kopf">
                      <WpButton
                        label="Gruppen ändern"
                        :icon="Users"
                        size="sm"
                        @press="bearbeiten(stand)"
                      />
                      <WpButton
                        :label="stand.geraet.gesperrt ? 'Entsperren' : 'Sperren'"
                        :icon="stand.geraet.gesperrt ? LockOpen : Lock"
                        size="sm"
                        :state="sperrt === stand.geraet.id ? 'busy' : 'default'"
                        @press="sperren(stand)"
                      />
                      <WpButton
                        label="Entfernen"
                        variant="danger"
                        :icon="Trash2"
                        size="sm"
                        @press="zuLoeschen = stand"
                      />
                    </div>

                    <p class="blass">
                      {{ revisionen(stand) }} · Client
                      {{ stand.meldung?.client_version ?? "—" }}
                    </p>

                    <p
                      v-if="wird_aktualisiert(stand)"
                      class="detail__hinweis"
                      role="status"
                    >
                      Der Server hat eine neue Revision; das Gerät übernimmt sie
                      beim nächsten Abgleich.
                    </p>

                    <p v-if="!stand.meldung" class="blass">
                      Das Gerät hat sich noch nicht gemeldet.
                    </p>
                    <p
                      v-else-if="stand.meldung.eintraege.length === 0"
                      class="blass"
                    >
                      Nichts zugewiesen: auf diesem Gerät läuft kein Eintrag.
                    </p>

                    <table
                      v-else
                      class="tabelle tabelle--innen"
                      :aria-label="`Zustand der Einträge auf ${geraet_name(stand)}`"
                    >
                      <thead>
                        <tr>
                          <th scope="col">Eintrag</th>
                          <th scope="col">Kennung</th>
                          <th scope="col">Zustand</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr
                          v-for="[id, eintragZustand] in zustand_eintraege(stand)"
                          :key="id"
                        >
                          <td>{{ titel(id) }}</td>
                          <td class="mono">{{ id }}</td>
                          <td>
                            <Status
                              :ton="zustand_ton(eintragZustand)"
                              :text="zustand_text(eintragZustand)"
                            />
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </Karte>

    <GeraetDialog
      v-model:offen="geraetDialogOffen"
      :stand="bearbeitet"
      :gruppen="gruppenListe"
      :client="client"
      @gespeichert="gespeichert"
    />

    <EinbindenDialog
      v-model:offen="einbindenOffen"
      :gruppen="gruppenListe"
      :client="client"
    />

    <WpDialog
      :open="zuLoeschen !== null"
      title="Gerät entfernen"
      :description="
        zuLoeschen
          ? `${geraet_name(zuLoeschen)} (${zuLoeschen.geraet.id}) wird entfernt. Das Gerät muss sich danach neu einbinden.`
          : ''
      "
      tone="error"
      @update:open="(wert) => (!wert ? schliesseLoeschen() : undefined)"
    >
      <p v-if="loeschfehler" class="fehler" role="alert">{{ loeschfehler }}</p>
      <template #actions>
        <WpButton label="Abbrechen" @press="schliesseLoeschen" />
        <WpButton
          label="Entfernen"
          variant="danger"
          :state="loescht ? 'busy' : 'default'"
          @press="entfernen"
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
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.meldung {
  display: flex;
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}

.tabelle__rahmen {
  overflow-x: auto;
}

.detail {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.detail__kopf {
  display: flex;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.detail__hinweis {
  padding-left: var(--wp-space-3);
  border-left: var(--wp-space-1) solid var(--wp-color-signal-default);
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.tabelle--innen {
  margin-top: var(--wp-space-1);
}
</style>
