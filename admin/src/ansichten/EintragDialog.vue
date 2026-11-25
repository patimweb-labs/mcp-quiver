<script setup lang="ts">
// Eintrag anlegen oder bearbeiten. Alle Angaben stehen im Entwurf; erst beim
// Speichern geht er zum Server. Die Laufzeit ist ein Summentyp – Stdio und
// Remote schließen einander aus, auch in der Oberfläche.
import { computed, ref, useId, watch } from "vue";
import { Plus } from "lucide-vue-next";
import {
  WpButton,
  WpDialog,
  WpInput,
  WpSwitch,
  WpTextarea,
  WpToggleGroup,
} from "@weppelmann/components";
import { tief_kopie } from "../logik/kopie";

import { ApiFehler, fehlertext, type Client } from "../api/client";
import type { Eintrag, Quelle } from "../api/typen";
import Feld from "../bausteine/Feld.vue";
import Feldkarte from "../bausteine/Feldkarte.vue";
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
  mehrfach_ist_leer,
  mehrfach_von,
  neuer_eintrag,
  neues_feld,
  neues_zugang_feld,
  quelle_text,
  umgebung_als_text,
  umgebung_aus_text,
  ZUGANG_QUELLEN,
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
  neuladen: [];
  gespeichert: [Eintrag];
}>();

const entwurf = ref<Eintrag>(neuer_eintrag());
const fehler = ref<Record<string, string>>({});
const speicherfehler = ref("");
const konflikt = ref(false);
const speichert = ref(false);
const idSelbstGesetzt = ref(false);
const adressId = useId();
const kopfzeilenId = useId();
const anmeldungId = useId();
const einrichtungId = useId();
const mehrfachId = useId();
// Der Abschnitt „Anmeldung“ ist zugeklappt, solange niemand ihn braucht.
const anmeldungOffen = ref(false);
// Dasselbe für die Einrichtung (Aufgabe 11).
const einrichtungOffen = ref(false);
// Und für die Zugangsliste (Aufgabe 14).
const mehrfachOffen = ref(false);

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
    mehrfachOffen.value = !mehrfach_ist_leer_entwurf();
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

// Eine Zugangsliste gehört dem Gerät: der Server füllt sie nie (Aufgabe 14).
const zugangQuelleOptionen = ZUGANG_QUELLEN.map((quelle) => ({
  value: quelle,
  label: quelle_text(quelle),
}));

function artSetzen(wert: string): void {
  if (wert === entwurf.value.laufzeit.art) return;
  entwurf.value.laufzeit =
    wert === "remote"
      ? { art: "remote", url: "", kopfzeilen: {} }
      : { art: "stdio", befehl: "", argumente: [], umgebung: {} };
  // Anmeldung, Einrichtung und Zugangsliste gibt es nur für Stdio-Einträge.
  if (entwurf.value.laufzeit.art === "remote") {
    entwurf.value.anmeldung = null;
    entwurf.value.einrichtung = null;
    entwurf.value.mehrfach = null;
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

/** Ist im Entwurf schon eine Zugangsliste angelegt? */
function mehrfach_ist_leer_entwurf(): boolean {
  return mehrfach_ist_leer(entwurf.value.mehrfach);
}

/** Die Zugangsliste als Daten; ohne Anlage ist sie leer. */
const mehrfach = computed(() => mehrfach_von(entwurf.value));

/** Eine Zugangsliste anlegen – mit einer sinnvollen leeren Vorlage. */
function mehrfachAnlegen(): void {
  entwurf.value.mehrfach = {
    titel: "Verbindungen",
    einzahl: "Verbindung",
    felder: [neues_zugang_feld()],
  };
  mehrfachOffen.value = true;
}

function mehrfachEntfernen(): void {
  entwurf.value.mehrfach = null;
  mehrfachOffen.value = false;
}

function zugangFeldHinzufuegen(): void {
  const liste = mehrfach_von(entwurf.value);
  entwurf.value.mehrfach = {
    ...liste,
    felder: [...liste.felder, neues_zugang_feld()],
  };
}

function zugangFeldEntfernen(index: number): void {
  const liste = mehrfach_von(entwurf.value);
  entwurf.value.mehrfach = {
    ...liste,
    felder: liste.felder.filter((_, i) => i !== index),
  };
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
    konflikt.value = fehlerWert instanceof ApiFehler && fehlerWert.status === 409;
    speicherfehler.value = konflikt.value
      ? "Inzwischen geändert – bitte neu laden."
      : fehlertext(fehlerWert);
  } finally {
    speichert.value = false;
  }
}

/** Nach einem 409: die Liste neu holen und den Dialog schließen. */
function neu_laden(): void {
  emit("neuladen");
  emit("update:offen", false);
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

        <Feldkarte
          v-for="(feld, i) in entwurf.felder"
          :key="i"
          :feld="feld"
          :nummer="i + 1"
          praefix="feld"
          :fehler="fehler"
          :quellen="quelleOptionen"
          :untertitel="quelle_text(feld.quelle)"
          @entfernen="feldEntfernen(i)"
        />
      </div>

      <section v-if="entwurf.laufzeit.art === 'stdio'" class="mehrfach">
        <Klappknopf
          :offen="mehrfachOffen"
          :steuert="mehrfachId"
          @umschalten="mehrfachOffen = !mehrfachOffen"
        >
          Mehrere Zugänge (optional)
        </Klappknopf>
        <p class="blass">
          Gleichartige Zugänge, die der User am Gerät anlegt – etwa
          SSH-Verbindungen. Der Client übergibt sie dem Tool beim Start als
          JSON in <code>QUIVER_ZUGAENGE</code>; Geheimes liegt im
          Tresor-Datei. Die Liste gehört dem Gerät, deshalb füllt sie nie der
          Server.
        </p>

        <div v-if="mehrfachOffen" :id="mehrfachId" class="mehrfach__inhalt">
          <p v-if="fehler.mehrfach" class="fehler" role="alert">
            {{ fehler.mehrfach }}
          </p>

          <template v-if="entwurf.mehrfach">
            <Feld
              beschriftung="Überschrift"
              :fehler="fehler['mehrfach.titel']"
              hinweis="Steht über der Liste im Client, z. B. „Verbindungen“."
            >
              <template #default="{ id }">
                <WpInput
                  :id="id"
                  :model-value="mehrfach.titel"
                  :state="fehler['mehrfach.titel'] ? 'invalid' : 'default'"
                  placeholder="Verbindungen"
                  @update:model-value="
                    (wert) => (entwurf.mehrfach = { ...mehrfach, titel: wert })
                  "
                />
              </template>
            </Feld>

            <Feld
              beschriftung="Bezeichnung"
              :fehler="fehler['mehrfach.einzahl']"
              hinweis="Ein Element der Liste, z. B. „Verbindung“."
            >
              <template #default="{ id }">
                <WpInput
                  :id="id"
                  :model-value="mehrfach.einzahl"
                  :state="fehler['mehrfach.einzahl'] ? 'invalid' : 'default'"
                  placeholder="Verbindung"
                  @update:model-value="
                    (wert) => (entwurf.mehrfach = { ...mehrfach, einzahl: wert })
                  "
                />
              </template>
            </Feld>

            <div class="mehrfach__kopf">
              <span class="mehrfach__titel">Felder eines Zugangs</span>
              <WpButton
                label="Feld hinzufügen"
                :icon="Plus"
                size="sm"
                @press="zugangFeldHinzufuegen"
              />
            </div>
            <p v-if="mehrfach.felder.length === 0" class="blass">
              Noch keine Felder. Ohne Feld weiß der Client nicht, was ein
              Zugang braucht.
            </p>

            <Feldkarte
              v-for="(feld, i) in mehrfach.felder"
              :key="i"
              :feld="feld"
              :nummer="i + 1"
              praefix="mehrfach.feld"
              :fehler="fehler"
              :quellen="zugangQuelleOptionen"
              :untertitel="quelle_text(feld.quelle)"
              @entfernen="zugangFeldEntfernen(i)"
            />

            <div>
              <WpButton label="Zugangsliste entfernen" @press="mehrfachEntfernen" />
            </div>
          </template>

          <div v-else>
            <WpButton
              label="Zugangsliste anlegen"
              :icon="Plus"
              size="sm"
              @press="mehrfachAnlegen"
            />
          </div>
        </div>
      </section>

      <p v-if="speicherfehler" class="fehler" role="alert">{{ speicherfehler }}</p>
    </div>

    <template #actions>
      <WpButton v-if="konflikt" label="Neu laden" @press="neu_laden" />
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

.mehrfach {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
  padding-top: var(--wp-space-3);
}

.mehrfach__inhalt {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-1);
}

.mehrfach__kopf {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--wp-space-2);
  flex-wrap: wrap;
}

.mehrfach__titel {
  font-size: var(--wp-font-size-lg);
  font-weight: var(--wp-font-weight-semibold);
}

</style>
