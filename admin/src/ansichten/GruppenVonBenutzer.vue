<script setup lang="ts">
// Der Abschnitt „Gruppen“ eines Benutzers (Aufgabe 33): was aus Entra kommt,
// was von Hand vergeben wurde und was am Gerät hängt – mit Hinzufügen und
// Entfernen. Die Gegenrichtung (Gruppe → Mitglieder) steht im Gruppen-Dialog;
// beide ändern dieselbe Zugehörigkeit.
import { computed, ref, shallowRef, watch } from "vue";
import { Trash2, Users } from "lucide-vue-next";
import { WpButton, WpDialog, WpIconButton } from "@weppelmann/components";
import type { ListOption } from "@weppelmann/components";

import { fehlertext, type Client } from "../api/client";
import type { BenutzerGruppe, BenutzerSicht, Gruppe } from "../api/typen";
import Mehrfachauswahl from "../bausteine/Mehrfachauswahl.vue";
import Status from "../bausteine/Status.vue";
import { zeitpunkt } from "../logik/format";
import {
  aussperr_warnung,
  entfernbar,
  fehlende_gruppen,
  gruppen_text,
  herkuenfte,
  letzte_gruppe,
} from "../logik/mitglieder";

const props = defineProps<{
  offen: boolean;
  benutzer: BenutzerSicht | null;
  alleGruppen: Gruppe[];
  client: Client;
  /** Objekt-Id der angemeldeten Person (für die Warnung beim Aussperren). */
  selbst: string | null;
}>();

const emit = defineEmits<{ "update:offen": [boolean]; geaendert: [] }>();

const liste = shallowRef<BenutzerGruppe[]>([]);
const gewaehlt = ref<string[]>([]);
const meldung = ref("");
const aktionsfehler = ref("");
const laeuft = ref(false);
const zu_entfernen = shallowRef<BenutzerGruppe | null>(null);

const selbst = computed(() => props.benutzer !== null && props.benutzer.oid === props.selbst);

const optionen = computed<ListOption[]>(() =>
  fehlende_gruppen(props.alleGruppen, liste.value).map((gruppe) => ({
    value: gruppe.id,
    label: gruppe.titel,
    hint: gruppe.id,
  })),
);

const warnung = computed(() => {
  const ziel = zu_entfernen.value;
  if (!ziel) return "";
  if (!letzte_gruppe(liste.value, ziel.gruppe)) return "";
  return aussperr_warnung(liste.value, selbst.value);
});

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    liste.value = props.benutzer?.gruppen ?? [];
    gewaehlt.value = [];
    meldung.value = "";
    aktionsfehler.value = "";
    zu_entfernen.value = null;
  },
  { immediate: true },
);

async function hinzufuegen(): Promise<void> {
  const benutzer = props.benutzer;
  if (!benutzer || gewaehlt.value.length === 0) return;
  laeuft.value = true;
  aktionsfehler.value = "";
  meldung.value = "";
  const anzahl = gewaehlt.value.length;
  try {
    for (const gruppe of [...gewaehlt.value]) {
      liste.value = await props.client.benutzerGruppeSetzen(benutzer.oid, gruppe, true);
    }
    gewaehlt.value = [];
    meldung.value = `${anzahl} ${anzahl === 1 ? "Gruppe" : "Gruppen"} hinzugefügt.`;
    emit("geaendert");
  } catch (fehlerWert) {
    aktionsfehler.value = fehlertext(fehlerWert);
  } finally {
    laeuft.value = false;
  }
}

async function entfernen(): Promise<void> {
  const benutzer = props.benutzer;
  const ziel = zu_entfernen.value;
  if (!benutzer || !ziel) return;
  laeuft.value = true;
  aktionsfehler.value = "";
  meldung.value = "";
  try {
    liste.value = await props.client.benutzerGruppeSetzen(
      benutzer.oid,
      ziel.gruppe,
      false,
      warnung.value.length > 0,
    );
    zu_entfernen.value = null;
    meldung.value = `Gruppe ${ziel.titel} entfernt.`;
    emit("geaendert");
  } catch (fehlerWert) {
    aktionsfehler.value = fehlertext(fehlerWert);
  } finally {
    laeuft.value = false;
  }
}
</script>

<template>
  <WpDialog
    :open="offen"
    :title="benutzer ? `Gruppen von ${benutzer.name ?? benutzer.email ?? benutzer.oid}` : 'Gruppen'"
    description="Aus Entra kommt, was in Entra steht; von Hand vergebene Gruppen bleiben bei der nächsten Auffrischung."
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div class="gruppen">
      <p v-if="meldung" class="gruppen__meldung" role="status">
        <Status ton="gut" :text="meldung" />
      </p>
      <p v-if="aktionsfehler" class="gruppen__fehler" role="alert">{{ aktionsfehler }}</p>

      <p class="gruppen__zaehler">{{ gruppen_text(liste.length) }}</p>

      <ul v-if="liste.length > 0" class="gruppen__liste">
        <li v-for="eintrag in liste" :key="eintrag.gruppe" class="gruppen__zeile">
          <Users class="gruppen__zeichen" aria-hidden="true" />
          <div class="gruppen__haupt">
            <div class="gruppen__namen">
              <span class="gruppen__name">{{ eintrag.titel }}</span>
              <span
                v-for="herkunft in herkuenfte(eintrag)"
                :key="herkunft.text"
                class="marke-chip"
                :title="herkunft.hinweis"
              >
                {{ herkunft.text }}
              </span>
            </div>
            <span class="gruppen__klein">{{ eintrag.gruppe }}</span>
            <span v-if="eintrag.von_hand && eintrag.seit" class="gruppen__klein">
              von Hand seit {{ zeitpunkt(eintrag.seit) }}
              <template v-if="eintrag.von"> · von {{ eintrag.von }}</template>
            </span>
            <span v-if="eintrag.aus_entra && eintrag.entra_ids.length > 0" class="gruppen__klein">
              Entra: {{ eintrag.entra_ids.join(", ") }} – ändern nur in Entra
            </span>
          </div>
          <div class="gruppen__aktionen">
            <WpIconButton
              v-if="entfernbar(eintrag)"
              :icon="Trash2"
              label="Von Hand entfernen"
              variant="ghost"
              @press="zu_entfernen = eintrag"
            />
          </div>
        </li>
      </ul>
      <p v-else class="gruppen__leer">
        Keine Gruppe – dieser Benutzer bekommt keine Tools, bis er in einer ist.
      </p>

      <div class="gruppen__hinzufuegen">
        <Mehrfachauswahl
          v-model="gewaehlt"
          :options="optionen"
          :label="`Gruppen hinzufügen (${optionen.length} zur Auswahl)`"
          placeholder="Gruppen auswählen …"
          suchbar
        />
        <WpButton
          label="Hinzufügen"
          :state="laeuft ? 'busy' : 'default'"
          @press="hinzufuegen"
        />
      </div>

      <p class="gruppen__klein">
        Von Hand vergebene Gruppen gehören dem Benutzer: sie überleben Auffrischung,
        Neuanmeldung und neue Geräte. Entfernen wirkt sofort.
      </p>
    </div>

    <template #actions>
      <WpButton label="Schließen" @press="emit('update:offen', false)" />
    </template>
  </WpDialog>

  <WpDialog
    :open="zu_entfernen !== null"
    title="Gruppe entfernen"
    :description="
      zu_entfernen
        ? `${zu_entfernen.titel} wird von Hand entnommen. Was aus Entra kommt, bleibt.` +
          (warnung ? ` ${warnung}` : '')
        : ''
    "
    :tone="warnung ? 'warn' : 'info'"
    @update:open="(wert) => (!wert ? (zu_entfernen = null) : undefined)"
  >
    <template #actions>
      <WpButton label="Abbrechen" @press="zu_entfernen = null" />
      <WpButton
        label="Entfernen"
        variant="danger"
        :state="laeuft ? 'busy' : 'default'"
        @press="entfernen"
      />
    </template>
  </WpDialog>
</template>

<style scoped>
.gruppen {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.gruppen__zaehler {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.gruppen__liste {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
  max-height: calc(var(--wp-space-8) * 7);
  overflow-y: auto;
}

.gruppen__zeile {
  display: flex;
  align-items: flex-start;
  gap: var(--wp-space-2);
  padding: var(--wp-space-2) 0;
  border-bottom: var(--wp-size-border) solid var(--wp-color-border-subtle);
}

.gruppen__zeile:last-child {
  border-bottom: 0;
}

.gruppen__zeichen {
  flex: none;
  width: var(--wp-space-4);
  height: var(--wp-space-4);
  margin-top: 2px;
  color: var(--wp-color-text-muted);
}

.gruppen__haupt {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.gruppen__namen {
  display: flex;
  align-items: center;
  gap: var(--wp-space-1);
  flex-wrap: wrap;
}

.gruppen__name {
  font-weight: var(--wp-font-weight-medium);
}

.gruppen__klein {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.gruppen__aktionen {
  display: flex;
  flex: none;
}

.gruppen__hinzufuegen {
  display: flex;
  align-items: flex-end;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.gruppen__hinzufuegen > :first-child {
  flex: 1 1 calc(var(--wp-space-8) * 4);
  min-width: 0;
}

.gruppen__leer {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.gruppen__meldung {
  display: flex;
}

.gruppen__fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
