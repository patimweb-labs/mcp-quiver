<script setup lang="ts">
// Eintrag anlegen oder bearbeiten. Alle Angaben stehen im Entwurf; erst beim
// Speichern geht er zum Server. Die Laufzeit ist ein Summentyp – Stdio und
// Remote schließen einander aus, auch in der Oberfläche.
import { computed, ref, useId, watch } from "vue";
import { Plus, Trash2 } from "lucide-vue-next";
import {
  WpButton,
  WpCheckbox,
  WpDialog,
  WpIconButton,
  WpInput,
  WpSelect,
  WpSwitch,
  WpTextarea,
  WpToggleGroup,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { fehlertext, type Client } from "../api/client";
import type { Eintrag, Quelle } from "../api/typen";
import Feld from "../bausteine/Feld.vue";
import {
  ANMELDEBEFEHLE,
  anmeldung_als_text,
  anmeldung_aus_text,
  anmeldung_von,
  argumente_als_text,
  argumente_aus_text,
  einrichtung_als_text,
  einrichtung_aus_text,
  einrichtung_von,
  eintrag_pruefen,
  eintrag_saeubern,
  kennung_aus_titel,
  neuer_eintrag,
  neues_feld,
  quelle_text,
  umgebung_als_text,
  umgebung_aus_text,
  type Anmeldebefehl,
} from "../logik/eintrag";
import Klappknopf from "../bausteine/Klappknopf.vue";

const props = defineProps<{
  offen: boolean;
  eintrag: Eintrag | null;
  neu: boolean;
  client: Client;
}>();

const emit = defineEmits<{
  "update:offen": [boolean];
  gespeichert: [Eintrag];
}>();

const entwurf = ref<Eintrag>(neuer_eintrag());
const fehler = ref<Record<string, string>>({});
const speicherfehler = ref("");
const speichert = ref(false);
const idSelbstGesetzt = ref(false);
const adressId = useId();
const kopfzeilenId = useId();
const anmeldungId = useId();
const einrichtungId = useId();
// Der Abschnitt „Anmeldung“ ist zugeklappt, solange niemand ihn braucht.
const anmeldungOffen = ref(false);
// Dasselbe für die Einrichtung (Aufgabe 11).
const einrichtungOffen = ref(false);

watch(
  () => props.offen,
  (offen) => {
    if (!offen) return;
    entwurf.value = props.eintrag
      ? tief_kopie(props.eintrag)
      : neuer_eintrag();
    fehler.value = {};
    speicherfehler.value = "";
    idSelbstGesetzt.value = !props.neu;
    anmeldungOffen.value = !anmeldung_ist_leer_entwurf();
    einrichtungOffen.value = !einrichtung_ist_leer_entwurf();
  },
  { immediate: true },
);

function titelAendern(wert: string): void {
  entwurf.value.titel = wert;
  if (!idSelbstGesetzt.value) entwurf.value.id = kennung_aus_titel(wert);
}

function idAendern(wert: string): void {
  entwurf.value.id = wert;
  idSelbstGesetzt.value = true;
}

const herkunftOptionen = [
  { value: "eigen", label: "Eigen" },
  { value: "fremd", label: "Fremd" },
];

const artOptionen = [
  { value: "stdio", label: "Stdio" },
  { value: "remote", label: "Remote" },
];

const quelleOptionen = (["server", "client", "geheim"] as Quelle[]).map(
  (quelle) => ({ value: quelle, label: quelle_text(quelle) }),
);

function artSetzen(wert: string): void {
  if (wert === entwurf.value.laufzeit.art) return;
  entwurf.value.laufzeit =
    wert === "remote"
      ? { art: "remote", url: "", kopfzeilen: {} }
      : { art: "stdio", befehl: "", argumente: [], umgebung: {} };
  // Eine Anmeldung gibt es nur für Stdio-Einträge.
  if (entwurf.value.laufzeit.art === "remote") {
    entwurf.value.anmeldung = null;
    entwurf.value.einrichtung = null;
  }
}

const befehl = computed(() => {
  const laufzeit = entwurf.value.laufzeit;
  return laufzeit.art === "stdio" ? laufzeit.befehl : "";
});

const url = computed(() => {
  const laufzeit = entwurf.value.laufzeit;
  return laufzeit.art === "remote" ? laufzeit.url : "";
});

function befehlSetzen(wert: string): void {
  const laufzeit = entwurf.value.laufzeit;
  if (laufzeit.art === "stdio") laufzeit.befehl = wert;
}

function urlSetzen(wert: string): void {
  const laufzeit = entwurf.value.laufzeit;
  if (laufzeit.art === "remote") laufzeit.url = wert;
}

const argumente = computed({
  get: () => {
    const laufzeit = entwurf.value.laufzeit;
    return laufzeit.art === "stdio"
      ? argumente_als_text(laufzeit.argumente)
      : "";
  },
  set: (text: string) => {
    const laufzeit = entwurf.value.laufzeit;
    if (laufzeit.art === "stdio") {
      laufzeit.argumente = argumente_aus_text(text);
    }
  },
});

const umgebung = computed({
  get: () => {
    const laufzeit = entwurf.value.laufzeit;
    return laufzeit.art === "stdio"
      ? umgebung_als_text(laufzeit.umgebung)
      : "";
  },
  set: (text: string) => {
    const laufzeit = entwurf.value.laufzeit;
    if (laufzeit.art === "stdio") {
      laufzeit.umgebung = umgebung_aus_text(text);
    }
  },
});

const kopfzeilen = computed({
  get: () => {
    const laufzeit = entwurf.value.laufzeit;
    return laufzeit.art === "remote"
      ? umgebung_als_text(laufzeit.kopfzeilen)
      : "";
  },
  set: (text: string) => {
    const laufzeit = entwurf.value.laufzeit;
    if (laufzeit.art === "remote") {
      laufzeit.kopfzeilen = umgebung_aus_text(text);
    }
  },
});

/** Ist im Entwurf schon eine Anmeldung angelegt? */
function anmeldung_ist_leer_entwurf(): boolean {
  const anmeldung = entwurf.value.anmeldung;
  return !anmeldung || ANMELDEBEFEHLE.every((b) => anmeldung[b].length === 0);
}

/** Die Anmeldung als Daten – ein leerer Abschnitt bleibt leer. */
const anmeldung = computed(() => anmeldung_von(entwurf.value));

/** Argumente eines Anmelde-Befehls im Feld (Leerzeichen-getrennt). */
function anmeldungText(befehl: Anmeldebefehl): string {
  return anmeldung_als_text(anmeldung.value[befehl]);
}

function anmeldungSetzen(befehl: Anmeldebefehl, text: string): void {
  const bisher = anmeldung_von(entwurf.value);
  entwurf.value.anmeldung = {
    ...bisher,
    [befehl]: anmeldung_aus_text(text),
  };
}

/** Ist im Entwurf schon eine Einrichtung angelegt? */
function einrichtung_ist_leer_entwurf(): boolean {
  const einrichtung = entwurf.value.einrichtung;
  return !einrichtung || einrichtung.einrichten.length === 0;
}

/** Die Argumente der Einrichtung im Feld (Leerzeichen-getrennt). */
const einrichtung = computed(() =>
  einrichtung_als_text(einrichtung_von(entwurf.value).einrichten),
);

function einrichtungSetzen(text: string): void {
  entwurf.value.einrichtung = { einrichten: einrichtung_aus_text(text) };
}

function feldHinzufuegen(): void {
  entwurf.value.felder.push(neues_feld());
}

function feldEntfernen(index: number): void {
  entwurf.value.felder.splice(index, 1);
}

function quelleSetzen(index: number, wert: string | null): void {
  if (wert === "server" || wert === "client" || wert === "geheim") {
    entwurf.value.felder[index].quelle = wert;
  }
}

/**
 * Link-Text und Link-Adresse eines Felds setzen (Aufgabe 13). Solange der
 * Link leer ist, steht dort ein leerer Text; `eintrag_saeubern` macht daraus
 * `null`.
 */
function linkSetzen(index: number, teil: "text" | "url", wert: string): void {
  const feld = entwurf.value.felder[index];
  const link = feld.link ?? { text: "", url: "" };
  feld.link = teil === "text" ? { ...link, text: wert } : { ...link, url: wert };
}

async function speichern(): Promise<void> {
  const sauber = eintrag_saeubern(entwurf.value);
  fehler.value = eintrag_pruefen(sauber);
  if (Object.keys(fehler.value).length > 0) return;

  speichert.value = true;
  speicherfehler.value = "";
  try {
    const gespeichert = await props.client.eintragSetzen(sauber);
    emit("gespeichert", gespeichert);
    emit("update:offen", false);
  } catch (fehlerWert) {
    speicherfehler.value = fehlertext(fehlerWert);
  } finally {
    speichert.value = false;
  }
}
</script>

<template>
  <WpDialog
    :open="offen"
    :title="neu ? 'Neuer Eintrag' : 'Eintrag bearbeiten'"
    description="Ein MCP-Tool, so wie der Client es startet."
    @update:open="(wert) => emit('update:offen', wert)"
  >
    <div class="form">
      <Feld
        beschriftung="Kennung"
        :fehler="fehler.id"
        hinweis="Präfix der Tool-Namen: kennung__tool"
      >
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="entwurf.id"
            :state="fehler.id ? 'invalid' : 'default'"
            placeholder="microsoft-mail"
            @update:model-value="idAendern"
          />
        </template>
      </Feld>

      <Feld beschriftung="Titel" :fehler="fehler.titel">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="entwurf.titel"
            :state="fehler.titel ? 'invalid' : 'default'"
            placeholder="Microsoft Mail"
            @update:model-value="titelAendern"
          />
        </template>
      </Feld>

      <Feld beschriftung="Beschreibung" hinweis="Was das Tool kann, in einem Satz.">
        <template #default="{ id }">
          <WpTextarea
            :id="id"
            :model-value="entwurf.beschreibung"
            :rows="2"
            @update:model-value="(wert) => (entwurf.beschreibung = wert)"
          />
        </template>
      </Feld>

      <Feld beschriftung="Version" :fehler="fehler.version">
        <template #default="{ id }">
          <WpInput
            :id="id"
            :model-value="entwurf.version"
            :state="fehler.version ? 'invalid' : 'default'"
            @update:model-value="(wert) => (entwurf.version = wert)"
          />
        </template>
      </Feld>

      <Feld
        beschriftung="Aktiv"
        hinweis="Abgeschaltete Einträge bleiben in Bibliothek und Sätzen, laufen aber auf keinem Gerät. Der Client beendet sie beim nächsten Abgleich."
        :benennt="false"
      >
        <WpSwitch
          :model-value="entwurf.aktiv"
          label="Auf Geräten ausführen"
          @update:model-value="(wert) => (entwurf.aktiv = wert)"
        />
      </Feld>

      <Feld
        beschriftung="Herkunft"
        hinweis="Fremde Einträge müssen freigegeben sein."
        :benennt="false"
      >
        <WpToggleGroup
          :model-value="entwurf.herkunft"
          :options="herkunftOptionen"
          size="sm"
          @update:model-value="
            (wert) => (entwurf.herkunft = wert === 'fremd' ? 'fremd' : 'eigen')
          "
        />
      </Feld>

      <Feld
        beschriftung="Laufzeit"
        hinweis="Stdio startet einen Prozess, Remote spricht Streamable HTTP."
        :benennt="false"
      >
        <WpToggleGroup
          :model-value="entwurf.laufzeit.art"
          :options="artOptionen"
          size="sm"
          @update:model-value="artSetzen"
        />
      </Feld>

      <template v-if="entwurf.laufzeit.art === 'stdio'">
        <Feld beschriftung="Befehl" :fehler="fehler.befehl">
          <template #default="{ id }">
            <WpInput
              :id="id"
              :model-value="befehl"
              :state="fehler.befehl ? 'invalid' : 'default'"
              placeholder="npx"
              @update:model-value="befehlSetzen"
            />
          </template>
        </Feld>

        <Feld beschriftung="Argumente" hinweis="Ein Argument je Zeile.">
          <template #default="{ id }">
            <WpTextarea :id="id" v-model="argumente" :rows="3" />
          </template>
        </Feld>

        <Feld
          beschriftung="Umgebung"
          hinweis="Je Zeile NAME=WERT. Platzhalter ${feld} sind erlaubt."
        >
          <template #default="{ id }">
            <WpTextarea :id="id" v-model="umgebung" :rows="3" />
          </template>
        </Feld>
      </template>

      <template v-if="entwurf.laufzeit.art === 'remote'">
        <Feld
          beschriftung="Adresse"
          :fehler="fehler.url"
          :kennung="adressId"
        >
          <template #default>
            <WpInput
              :id="adressId"
              :model-value="url"
              :state="fehler.url ? 'invalid' : 'default'"
              placeholder="https://example.org/mcp"
              @update:model-value="urlSetzen"
            />
          </template>
        </Feld>

        <Feld
          beschriftung="Kopfzeilen"
          hinweis="Je Zeile NAME=WERT. Platzhalter ${feld} sind erlaubt."
          :kennung="kopfzeilenId"
        >
          <template #default>
            <WpTextarea :id="kopfzeilenId" v-model="kopfzeilen" :rows="3" />
          </template>
        </Feld>
      </template>

      <section
        v-if="entwurf.laufzeit.art === 'stdio'"
        class="anmeldung"
      >
        <Klappknopf
          :offen="anmeldungOffen"
          :steuert="anmeldungId"
          @umschalten="anmeldungOffen = !anmeldungOffen"
        >
          Anmeldung (optional)
        </Klappknopf>
        <p class="blass">
          Braucht das Tool eine Anmeldung des Users, löst er sie im
          Client-Fenster aus – nie ein Agent. Alle drei Befehle sind
          Argumente für dasselbe Programm wie die Laufzeit.
        </p>

        <div v-if="anmeldungOffen" :id="anmeldungId" class="anmeldung__felder">
          <p v-if="fehler.anmeldung" class="fehler" role="alert">
            {{ fehler.anmeldung }}
          </p>
          <Feld
            beschriftung="Anmelden"
            :fehler="fehler['anmeldung.anmelden']"
            hinweis="Argumente, Leerzeichen-getrennt. Der Befehl öffnet den Browser; Exit 0 = angemeldet."
          >
            <template #default="{ id }">
              <WpInput
                :id="id"
                :model-value="anmeldungText('anmelden')"
                :state="fehler['anmeldung.anmelden'] ? 'invalid' : 'default'"
                placeholder="anmelden"
                @update:model-value="(wert) => anmeldungSetzen('anmelden', wert)"
              />
            </template>
          </Feld>

          <Feld
            beschriftung="Status"
            :fehler="fehler['anmeldung.status']"
            hinweis="Exit 0 = angemeldet (erste Zeile von stdout ist das Konto), Exit 1 = abgemeldet."
          >
            <template #default="{ id }">
              <WpInput
                :id="id"
                :model-value="anmeldungText('status')"
                :state="fehler['anmeldung.status'] ? 'invalid' : 'default'"
                placeholder="status"
                @update:model-value="(wert) => anmeldungSetzen('status', wert)"
              />
            </template>
          </Feld>

          <Feld
            beschriftung="Abmelden"
            :fehler="fehler['anmeldung.abmelden']"
            hinweis="Exit 0 = abgemeldet. Danach startet der Client den Eintrag neu."
          >
            <template #default="{ id }">
              <WpInput
                :id="id"
                :model-value="anmeldungText('abmelden')"
                :state="fehler['anmeldung.abmelden'] ? 'invalid' : 'default'"
                placeholder="abmelden"
                @update:model-value="(wert) => anmeldungSetzen('abmelden', wert)"
              />
            </template>
          </Feld>
        </div>
      </section>

      <section v-if="entwurf.laufzeit.art === 'stdio'" class="einrichtung">
        <Klappknopf
          :offen="einrichtungOffen"
          :steuert="einrichtungId"
          @umschalten="einrichtungOffen = !einrichtungOffen"
        >
          Einrichtung (optional)
        </Klappknopf>
        <p class="blass">
          Verwalten Tools ihre Zugänge in Profilen, öffnet der Client die Seite
          des Tools im Browser – nie ein Agent. Die Argumente gelten für
          dasselbe Programm wie die Laufzeit. Exit 0 = eingerichtet, der
          Eintrag startet danach neu; Exit 1 = abgebrochen.
        </p>

        <div v-if="einrichtungOffen" :id="einrichtungId" class="einrichtung__felder">
          <p v-if="fehler.einrichtung" class="fehler" role="alert">
            {{ fehler.einrichtung }}
          </p>
          <Feld
            beschriftung="Argumente"
            :fehler="fehler['einrichtung.einrichten']"
            hinweis="Leerzeichen-getrennt, z. B. „einrichten“. Platzhalter ${feld} sind erlaubt."
          >
            <template #default="{ id }">
              <WpInput
                :id="id"
                :model-value="einrichtung"
                :state="fehler['einrichtung.einrichten'] ? 'invalid' : 'default'"
                placeholder="einrichten"
                @update:model-value="einrichtungSetzen"
              />
            </template>
          </Feld>
        </div>
      </section>

      <div class="felder">
        <div class="felder__kopf">
          <span class="felder__titel">Felder</span>
          <WpButton
            label="Feld hinzufügen"
            :icon="Plus"
            size="sm"
            @press="feldHinzufuegen"
          />
        </div>
        <p v-if="entwurf.felder.length === 0" class="blass">
          Noch keine Felder. Ohne Feld braucht der Eintrag keine Angaben.
        </p>

        <fieldset
          v-for="(feld, i) in entwurf.felder"
          :key="i"
          class="feldkarte"
        >
          <legend class="feldkarte__titel">Feld {{ i + 1 }}</legend>
          <div class="feldkarte__kopf">
            <span class="blass">{{ quelle_text(feld.quelle) }}</span>
            <WpIconButton
              :icon="Trash2"
              label="Feld entfernen"
              size="sm"
              variant="ghost"
              @press="feldEntfernen(i)"
            />
          </div>

          <Feld beschriftung="Name" :fehler="fehler[`feld.${i}.name`]">
            <template #default="{ id }">
              <WpInput
                :id="id"
                :model-value="feld.name"
                :state="fehler[`feld.${i}.name`] ? 'invalid' : 'default'"
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
            <WpSelect
              :model-value="feld.quelle"
              :options="quelleOptionen"
              label="Quelle"
              @update:model-value="(wert) => quelleSetzen(i, wert)"
            />
            <WpCheckbox
              :model-value="feld.pflicht"
              label="Pflicht"
              @update:model-value="(wert) => (feld.pflicht = wert)"
            />
          </div>

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
                  @update:model-value="(wert) => linkSetzen(i, 'text', wert)"
                />
              </template>
            </Feld>

            <Feld
              beschriftung="Link-Adresse"
              hinweis="Platzhalter ${feld} sind erlaubt."
            >
              <template #default="{ id }">
                <WpInput
                  :id="id"
                  :model-value="feld.link?.url ?? ''"
                  placeholder="${url}/settings/api-and-access"
                  @update:model-value="(wert) => linkSetzen(i, 'url', wert)"
                />
              </template>
            </Feld>
          </div>
        </fieldset>
      </div>

      <p v-if="speicherfehler" class="fehler" role="alert">{{ speicherfehler }}</p>
    </div>

    <template #actions>
      <WpButton label="Abbrechen" @press="emit('update:offen', false)" />
      <WpButton
        label="Speichern"
        variant="primary"
        :state="speichert ? 'busy' : 'default'"
        @press="speichern"
      />
    </template>
  </WpDialog>
</template>

<style scoped>
.form {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  max-height: calc(var(--wp-space-8) * 9);
  overflow-y: auto;
  padding-right: var(--wp-space-1);
}

.anmeldung {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  padding-top: var(--wp-space-3);
}

.anmeldung__felder {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
}
.einrichtung {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  padding-top: var(--wp-space-3);
}

.einrichtung__felder {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
}

.felder {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
  padding-top: var(--wp-space-3);
}

.felder__kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.felder__titel {
  font-size: var(--wp-font-size-lg);
  font-weight: var(--wp-font-weight-semibold);
}

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

.fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}
</style>
