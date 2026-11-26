<script setup lang="ts">
// Überblick: Zähler aus `/admin/ueberblick`, dazu die Geräte, die Aufmerk-
// samkeit brauchen (Fehler oder fehlende Angaben).
import { onMounted, shallowRef } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { WpButton } from "@weppelmann/components";

import type { Client } from "../api/client";
import { fehlertext } from "../api/client";
import type {
  Eintrag,
  GeraetStand,
  Ueberblick,
  UpdateStand,
} from "../api/typen";
import Karte from "../bausteine/Karte.vue";
import Kasten from "../bausteine/Kasten.vue";
import Leer from "../bausteine/Leer.vue";
import Seitenkopf from "../bausteine/Seitenkopf.vue";
import Status from "../bausteine/Status.vue";
import {
  geraete_mit_problemen,
  geraet_name,
  problem_texte,
  stand_text,
  wird_aktualisiert,
} from "../logik/geraet";
import { nach_kennung } from "../logik/eintrag";
import { zeitpunkt } from "../logik/format";
import { type Zustand, fehler, fertig, laedt } from "../logik/seite";
import {
  abfrage_text,
  neueste_version,
  stand_hinweis,
  veraltete_text,
} from "../logik/update";
import { kennzahlen } from "../logik/ueberblick";

interface Daten {
  ueberblick: Ueberblick;
  geraete: GeraetStand[];
  titel: Map<string, string>;
  /** Stand der Update-Prüfung (Aufgabe 34); `null`, wenn nicht abrufbar. */
  updates: UpdateStand | null;
}

const props = defineProps<{ client: Client }>();
const zustand = shallowRef<Zustand<Daten>>(laedt());

async function laden(): Promise<void> {
  zustand.value = laedt();
  try {
    // Der Update-Stand darf fehlen (älterer Server): die Übersicht bleibt
    // dann nutzbar.
    const [ueberblick, geraete, eintraege, updates] = await Promise.all([
      props.client.ueberblick(),
      props.client.geraete(),
      props.client.eintraege(),
      props.client.updates().catch(() => null),
    ]);
    zustand.value = fertig({
      ueberblick,
      geraete,
      titel: titel_map(eintraege),
      updates,
    });
  } catch (fehlerWert) {
    zustand.value = fehler(fehlertext(fehlerWert));
  }
}

function titel_map(eintraege: Eintrag[]): Map<string, string> {
  const nach = nach_kennung(eintraege);
  return new Map([...nach].map(([id, eintrag]) => [id, eintrag.titel]));
}

function problem_titel(id: string): string {
  return zustand.value.art === "fertig"
    ? zustand.value.daten.titel.get(id) ?? id
    : id;
}

onMounted(laden);
</script>

<template>
  <div class="seite">
    <Seitenkopf
      titel="Überblick"
      unter="Was der Server kennt und wo es noch hakt."
    />

    <template v-if="zustand.art === 'laedt'">
      <Leer titel="Lädt …" erklaerung="Der Server wird gefragt." />
    </template>

    <template v-else-if="zustand.art === 'fehler'">
      <Karte titel="Nicht geladen" :unter="zustand.text">
        <div class="seite__aktion">
          <WpButton
            label="Neu laden"
            :icon="RefreshCw"
            @press="laden"
          />
        </div>
      </Karte>
    </template>

    <template v-else>
      <div class="zahlen">
        <Kasten v-for="zahl in kennzahlen(zustand.daten.ueberblick)" :key="zahl.schluessel">
          <p class="zahlen__titel">{{ zahl.titel }}</p>
          <p class="zahlen__wert">{{ zahl.wert }}</p>
        </Kasten>
      </div>

      <Karte
        titel="Geräte mit Problemen"
        unter="Fehler oder fehlende Angaben am Gerät."
      >
        <template #aktionen>
          <WpButton label="Neu laden" :icon="RefreshCw" size="sm" @press="laden" />
        </template>

        <div
          v-if="geraete_mit_problemen(zustand.daten.geraete).length === 0"
          class="seite__leer"
        >
          <Status ton="gut" text="Keine Probleme gemeldet." />
        </div>

        <ul v-else class="probleme">
          <li
            v-for="stand in geraete_mit_problemen(zustand.daten.geraete)"
            :key="stand.geraet.id"
            class="problem"
          >
            <div class="problem__kopf">
              <span class="problem__name">{{ geraet_name(stand) }}</span>
              <Status
                v-if="wird_aktualisiert(stand)"
                ton="warnung"
                text="wird aktualisiert"
              />
              <Status v-else ton="leise" :text="stand_text(stand)" />
            </div>
            <ul class="problem__liste">
              <li
                v-for="text in problem_texte(stand, problem_titel)"
                :key="text"
                class="problem__zeile"
              >
                {{ text }}
              </li>
            </ul>
          </li>
        </ul>
      </Karte>

      <Karte
        v-if="zustand.daten.updates"
        titel="Updates"
        :unter="`Abfrage: ${abfrage_text(zustand.daten.updates)}`"
      >
        <p
          v-if="stand_hinweis(zustand.daten.updates)"
          class="problem__zeile"
          role="status"
        >
          {{ stand_hinweis(zustand.daten.updates) }}
        </p>
        <dl class="fakten">
          <dt class="fakten__key">Neueste Client-Version</dt>
          <dd class="fakten__wert mono">
            {{ neueste_version(zustand.daten.updates) || "—" }}
          </dd>
          <dt class="fakten__key">Zuletzt gefragt</dt>
          <dd class="fakten__wert">
            {{
              zustand.daten.updates.zuletzt_geprueft
                ? zeitpunkt(zustand.daten.updates.zuletzt_geprueft)
                : "noch nicht"
            }}
          </dd>
        </dl>
        <p
          v-if="veraltete_text(zustand.daten.updates)"
          class="problem__zeile"
          role="status"
        >
          {{ veraltete_text(zustand.daten.updates) }}
        </p>
        <ul v-if="zustand.daten.updates.veraltete_clients.length" class="problem__liste">
          <li
            v-for="eintrag in zustand.daten.updates.veraltete_clients"
            :key="eintrag.geraet"
            class="problem__zeile"
          >
            {{ eintrag.name || eintrag.geraet }} – Client {{ eintrag.version }}
          </li>
        </ul>
        <p class="problem__zeile">
          Eingespielt wird durch die Geräteverwaltung oder von Hand aus dem
          Release; siehe docs/intune.md.
        </p>
      </Karte>

      <Karte titel="Server">
        <dl class="fakten">
          <dt class="fakten__key">Version</dt>
          <dd class="fakten__wert mono">{{ zustand.daten.ueberblick.server_version }}</dd>
          <dt class="fakten__key">Geräte verbunden</dt>
          <dd class="fakten__wert">
            {{ zustand.daten.ueberblick.geraete_verbunden }} von
            {{ zustand.daten.ueberblick.geraete }}
          </dd>
        </dl>
      </Karte>
    </template>
  </div>
</template>

<style scoped>
.seite {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-4);
}

.seite__aktion {
  display: flex;
  gap: var(--wp-space-2);
}

.seite__leer {
  padding-block: var(--wp-space-2);
}

.zahlen {
  display: grid;
  grid-template-columns: repeat(
    auto-fit,
    minmax(calc(var(--wp-space-8) * 2.5), 1fr)
  );
  gap: var(--wp-space-4);
}

.zahlen__titel {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.zahlen__wert {
  font-family: var(--wp-font-family-mono);
  font-size: var(--wp-font-size-2xl);
  line-height: var(--wp-font-line-tight);
}

.probleme {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.problem {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  padding-left: var(--wp-space-3);
  border-left: var(--wp-space-1) solid var(--wp-color-signal-default);
}

.problem__kopf {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.problem__name {
  font-weight: var(--wp-font-weight-semibold);
}

.problem__liste {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.problem__zeile {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.fakten {
  display: grid;
  grid-template-columns: calc(var(--wp-space-8) * 2) 1fr;
  gap: var(--wp-space-1) var(--wp-space-3);
  font-size: var(--wp-font-size-sm);
}

.fakten__key {
  color: var(--wp-color-text-faint);
}

.fakten__wert {
  min-width: 0;
  overflow-wrap: anywhere;
}
</style>
