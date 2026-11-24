<script setup lang="ts">
// Gruppen: Sätze zuordnen. Gruppen aus Entra ID legt der Server bei der ersten
// Anmeldung selbst an (Aufgabe 23); sie sind gekennzeichnet, ihr Titel und
// ihre Sätze gehören dem Admin. Gelöschte Entra-Gruppen lassen sich hier
// wieder aufnehmen.
import { onMounted, ref, shallowRef } from "vue";
import { Pencil, Plus, RefreshCw, Trash2, Users } from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpIconButton,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { fehlertext, type Client } from "../api/client";
import type { EntraGruppe, Gruppe, Satz } from "../api/typen";
import Karte from "../bausteine/Karte.vue";
import Kasten from "../bausteine/Kasten.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import {
  entra_name_hinweis,
  gruppe_hinweis,
  ist_entra_gruppe,
} from "../logik/ausweis";
import { neue_gruppe, saetze_sortiert } from "../logik/satz";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";
import GruppeDialog from "./GruppeDialog.vue";

const props = defineProps<{ client: Client }>();

interface Daten {
  gruppen: Gruppe[];
  saetze: Satz[];
  /** Alle gesehenen Entra-Gruppen; `name_entra` ist die kleine Zeile (Aufgabe 30). */
  gesehen: EntraGruppe[];
  /** Gelöschte Entra-Gruppen: bleiben gelöscht, bis der Admin sie aufnimmt. */
  geloeschte: EntraGruppe[];
}

const zustand = shallowRef<Zustand<Daten>>(laedt());
const meldung = ref("");
const dialogOffen = ref(false);
const neuModus = ref(false);
const bearbeitet = shallowRef<Gruppe | null>(null);

const zuLoeschen = shallowRef<Gruppe | null>(null);
const loeschfehler = ref("");
const loescht = ref(false);

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    const [gruppen, saetze, entra] = await Promise.all([
      props.client.gruppen(),
      props.client.saetze(),
      // Ältere Server kennen den Weg nicht; das ist kein Grund, die Seite
      // nicht zu zeigen.
      props.client.entraGruppen().catch(() => [] as EntraGruppe[]),
    ]);
    zustand.value = fertig({
      gruppen: [...gruppen].sort((a, b) =>
        a.titel.localeCompare(b.titel, "de", { sensitivity: "base" }),
      ),
      saetze: saetze_sortiert(saetze),
      gesehen: entra,
      geloeschte: entra.filter((eintrag) => eintrag.geloescht),
    });
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

function neu(): void {
  meldung.value = "";
  neuModus.value = true;
  bearbeitet.value = neue_gruppe();
  dialogOffen.value = true;
}

function bearbeiten(gruppe: Gruppe): void {
  meldung.value = "";
  neuModus.value = false;
  bearbeitet.value = tief_kopie(gruppe);
  dialogOffen.value = true;
}

async function gespeichert(gruppe: Gruppe): Promise<void> {
  meldung.value = `Gruppe ${gruppe.id} gespeichert.`;
  await laden();
}

async function aufnehmen(id: string): Promise<void> {
  meldung.value = "";
  try {
    await props.client.entraGruppeAufnehmen(id);
    meldung.value = `Entra-Gruppe ${id} wieder aufgenommen; sie hat noch keinen Satz.`;
    await laden();
  } catch (fehlerWert) {
    meldung.value = fehlertext(fehlerWert);
  }
}

async function entfernen(): Promise<void> {
  const ziel = zuLoeschen.value;
  if (!ziel) return;
  loescht.value = true;
  loeschfehler.value = "";
  try {
    await props.client.gruppeLoeschen(ziel.id);
    zuLoeschen.value = null;
    meldung.value = `Gruppe ${ziel.id} entfernt.`;
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

function satz_titel(id: string): string {
  if (zustand.value.art !== "fertig") return id;
  return zustand.value.daten.saetze.find((satz) => satz.id === id)?.titel ?? id;
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Gruppen"
      unter="Wer welche Sätze bekommt; Geräte gehören zu Gruppen."
    />

    <div class="werkzeuge">
      <WpButton label="Neu" variant="primary" :icon="Plus" @press="neu" />
      <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
    </div>

    <p v-if="meldung" class="meldung" role="status">
      <Status ton="gut" :text="meldung" />
    </p>

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Gruppen und Sätze werden geholt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <WpButton label="Neu laden" :icon="RefreshCw" @press="laden" />
      </Karte>
    </template>

    <template v-else-if="zustand.daten.gruppen.length === 0">
      <Leer
        titel="Noch keine Gruppen"
        erklaerung="Eine Gruppe verbindet Sätze mit Geräten."
      >
        <template #aktion>
          <WpButton label="Neu" variant="primary" :icon="Plus" @press="neu" />
        </template>
      </Leer>
    </template>

    <Kasten v-else>
      <div class="liste">
        <article
          v-for="gruppe in zustand.daten.gruppen"
          :key="gruppe.id"
          class="zeile"
        >
          <Users class="zeile__zeichen" aria-hidden="true" />
          <div class="zeile__haupt">
            <div class="zeile__kopf">
              <h3 class="zeile__titel" :title="`${gruppe.titel} · ${gruppe.id}`">
                {{ gruppe.titel }}
              </h3>
              <span v-if="ist_entra_gruppe(gruppe)" class="marke-chip">
                Aus Entra
              </span>
              <span class="marke-chip">
                {{ gruppe.saetze.length }}
                {{ gruppe.saetze.length === 1 ? "Satz" : "Sätze" }}
              </span>
            </div>
            <p v-if="gruppe.saetze.length > 0" class="blass">
              {{ gruppe.saetze.map((id) => satz_titel(id)).join(", ") }}
            </p>
            <p
              v-if="entra_name_hinweis(gruppe, zustand.daten.gesehen)"
              class="blass klein"
            >
              {{ entra_name_hinweis(gruppe, zustand.daten.gesehen) }}
            </p>
            <p v-if="gruppe_hinweis(gruppe)" class="blass">
              {{ gruppe_hinweis(gruppe) }}
            </p>
          </div>
          <div class="zeile__aktionen">
            <WpIconButton
              :icon="Pencil"
              label="Bearbeiten"
              @press="bearbeiten(gruppe)"
            />
            <WpIconButton
              :icon="Trash2"
              label="Entfernen"
              variant="ghost"
              @press="zuLoeschen = gruppe"
            />
          </div>
        </article>
      </div>
    </Kasten>

    <Karte
      v-if="zustand.art === 'fertig' && zustand.daten.geloeschte.length > 0"
      titel="Gelöschte Entra-Gruppen"
      unter="Sie bleiben gelöscht, bis Sie sie wieder aufnehmen; Sätze sind dann keine zugeordnet."
    >
      <div class="liste">
        <article
          v-for="eintrag in zustand.daten.geloeschte"
          :key="eintrag.id"
          class="zeile"
        >
          <Users class="zeile__zeichen" aria-hidden="true" />
          <div class="zeile__haupt">
            <p :title="eintrag.id">{{ eintrag.name_entra ?? eintrag.id }}</p>
            <p v-if="eintrag.name_entra" class="mono blass klein">
              {{ eintrag.id }}
            </p>
            <p class="blass klein">
              Zuletzt gesehen: {{ eintrag.zuletzt }}
            </p>
          </div>
          <div class="zeile__aktionen">
            <WpButton
              label="Wieder aufnehmen"
              @press="aufnehmen(eintrag.id)"
            />
          </div>
        </article>
      </div>
    </Karte>

    <GruppeDialog
      v-model:offen="dialogOffen"
      :gruppe="bearbeitet"
      :neu="neuModus"
      :satze="zustand.art === 'fertig' ? zustand.daten.saetze : []"
      :client="client"
      @gespeichert="gespeichert"
      @neuladen="laden"
    />

    <WpDialog
      :open="zuLoeschen !== null"
      title="Gruppe entfernen"
      :description="
        zuLoeschen
          ? `${zuLoeschen.titel} (${zuLoeschen.id}) wird entfernt. Geräte verlieren damit diese Zuweisung.` +
            (ist_entra_gruppe(zuLoeschen)
              ? ' Diese Gruppe stammt aus Entra und bleibt gelöscht, bis Sie sie wieder aufnehmen.'
              : '')
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

.liste {
  display: flex;
  flex-direction: column;
}

.zeile {
  display: flex;
  align-items: flex-start;
  gap: var(--wp-space-3);
  padding-block: var(--wp-space-3);
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.zeile:first-child {
  padding-top: 0;
}

.zeile:last-child {
  padding-bottom: 0;
  border-bottom: 0;
}

.zeile__zeichen {
  flex: none;
  width: var(--wp-font-size-lg);
  height: var(--wp-font-size-lg);
  margin-top: var(--wp-space-1);
  color: var(--wp-color-text-muted);
}

.zeile__haupt {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--wp-space-1);
  min-width: 0;
}

.zeile__kopf {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.zeile__titel {
  font-size: var(--wp-font-size-md);
  font-weight: var(--wp-font-weight-semibold);
}

.zeile__aktionen {
  display: flex;
  gap: var(--wp-space-1);
  flex: none;
}

.klein {
  font-size: var(--wp-font-size-sm);
}

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
