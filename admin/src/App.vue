<script setup lang="ts">
// Die Verwaltung: Anmeldung, Kopf, Navigation links, Seite rechts.
//
// Alles, was die Seite ausmacht, steht im Modell: die Lage vor der Anmeldung
// (`Lage` aus `logik/anmeldung.ts`), das Theme und die Seite. Die Ansichten
// bekommen den Client und zeichnen; ihre eigenen Zustände sind Summentypen
// (`Zustand`).
//
// Seit ADR 0002 gibt es keinen Admin-Token mehr: die Sitzung liegt allein im
// HttpOnly-Cookie. Nichts davon steht im Speicher des Browsers – nur das Theme.
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import type { Ich, Seite, Thema } from "./api/typen";
import {
  Client,
  abmelden as sitzungBeenden,
  anmeldearten,
  aufNichtAngemeldet,
  fehlertext,
  ich as wer_bin_ich,
  stand,
} from "./api/client";
import type { Lage } from "./logik/anmeldung";
import {
  anmeldung_lage,
  lagetitel,
  lage_aus_ich,
  lage_aus_stand,
  lage_nach_401,
  lage_ohne_stand,
  weiter_nach_anmeldung,
} from "./logik/anmeldung";
import { hash_aus_seite, seite_aus_hash, seite_titel } from "./logik/navigation";
import { thema_aus_wert, THEMA_SPEICHER } from "./logik/thema";

import Anmeldebereich from "./ansichten/Anmeldebereich.vue";
import Benutzer from "./ansichten/Benutzer.vue";
import Bibliothek from "./ansichten/Bibliothek.vue";
import Geraete from "./ansichten/Geraete.vue";
import Gruppen from "./ansichten/Gruppen.vue";
import Pruefspur from "./ansichten/Pruefspur.vue";
import Saetze from "./ansichten/Saetze.vue";
import Ueberblick from "./ansichten/Ueberblick.vue";
import Kopf from "./bausteine/Kopf.vue";
import Seitenleiste from "./bausteine/Seitenleiste.vue";

const thema = ref<Thema>(thema_aus_wert(localStorage.getItem(THEMA_SPEICHER)));
const seite = ref<Seite>(seite_aus_hash(window.location.hash));
/** Wohin es nach der Anmeldung geht (nur eigene Seiten). */
const weiter = ref<Seite>(weiter_nach_anmeldung(window.location.hash));
const lage = ref<Lage>({ art: "laedt" });
/** Kennt der Server Microsoft? Steht im Stand und färbt den 401-Hinweis. */
const kennt_entra = ref(false);

/** Ein Client, kein Token: die Sitzung schickt der Browser als Cookie mit. */
const client = new Client();

const ANSICHTEN = {
  ueberblick: Ueberblick,
  bibliothek: Bibliothek,
  saetze: Saetze,
  gruppen: Gruppen,
  benutzer: Benutzer,
  geraete: Geraete,
  pruefspur: Pruefspur,
} as const;

const ansicht = computed(() => ANSICHTEN[seite.value]);

function setzeThema(wert: string): void {
  thema.value = thema_aus_wert(wert);
}

watch(
  thema,
  (wert) => {
    document.documentElement.dataset.theme = wert;
    localStorage.setItem(THEMA_SPEICHER, wert);
  },
  { immediate: true },
);

watch(
  seite,
  (wert) => {
    const ziel = hash_aus_seite(wert);
    if (window.location.hash !== ziel) {
      window.history.replaceState(null, "", ziel);
    }
  },
  { immediate: true },
);

// Der Fenstertitel folgt dem Modell: vor der Anmeldung die Lage, danach die
// Seite. Eine Seite ohne Namen wäre für Screenreader und Verlauf nichts.
watch(
  [seite, lage],
  () => {
    const titel =
      lage.value.art === "bereit"
        ? seite_titel(seite.value)
        : lagetitel(lage.value);
    document.title = `${titel} · mcp-quiver`;
  },
  { immediate: true },
);

function beimHash(): void {
  seite.value = seite_aus_hash(window.location.hash);
}

function waehleSeite(wert: Seite): void {
  seite.value = wert;
}

/**
 * Der erste Blick: Gibt es einen Root (dann anmelden, vielleicht ist die
 * Sitzung noch gültig), oder ist die Ersteinrichtung offen? Ein Server ohne
 * `/auth/stand` (älter als Aufgabe 25) kennt nur die Anmeldung.
 */
async function starten(): Promise<void> {
  lage.value = { art: "laedt" };
  try {
    const serverstand = await stand();
    let basis: Lage;
    if (serverstand) {
      kennt_entra.value = serverstand.entra;
      basis = lage_aus_stand(serverstand);
    } else {
      const arten = await anmeldearten();
      kennt_entra.value = arten.entra;
      basis = lage_ohne_stand(arten);
    }
    if (basis.art === "anmeldung") {
      const angemeldet = await wer_bin_ich();
      if (angemeldet) {
        lage.value = lage_aus_ich(angemeldet);
        return;
      }
    }
    lage.value = basis;
  } catch (fehler) {
    lage.value = { art: "fehler", text: fehlertext(fehler) };
  }
}

/** Nach Anmelden oder Einrichten: in die Verwaltung, auf die gemeinte Seite. */
function angemeldet(ich: Ich): void {
  seite.value = weiter.value;
  lage.value = lage_aus_ich(ich);
}

async function abmelden(): Promise<void> {
  try {
    await sitzungBeenden();
  } catch (fehler) {
    // Die Sitzung ist auch ohne den Server zu erreichen weg; kein Grund,
    // die Anmeldung zu verweigern.
    console.warn("Abmelden am Server gescheitert:", fehlertext(fehler));
  }
  lage.value = anmeldung_lage(kennt_entra.value);
}

onMounted(() => {
  window.addEventListener("hashchange", beimHash);
  // Ein 401 heißt: die Sitzung gilt nicht mehr. Also zurück zur Anmeldung,
  // mit Begründung und mit dem Ziel, auf dem wir waren – aber nur, wenn es
  // eine eigene Seite ist (`weiter_nach_anmeldung`).
  aufNichtAngemeldet(() => {
    weiter.value = weiter_nach_anmeldung(window.location.hash);
    lage.value = lage_nach_401(kennt_entra.value);
  });
  void starten();
});

onBeforeUnmount(() => {
  window.removeEventListener("hashchange", beimHash);
  aufNichtAngemeldet(null);
});
</script>

<template>
  <div class="wp-root app">
    <template v-if="lage.art === 'bereit'">
      <Kopf
        :thema="thema"
        :ich="lage.ich"
        @thema="setzeThema"
        @abmelden="abmelden"
      />
      <div class="app__koerper">
        <Seitenleiste :seite="seite" @waehle="waehleSeite" />
        <main class="app__inhalt">
          <component :is="ansicht" :client="client" />
        </main>
      </div>
    </template>

    <Anmeldebereich
      v-else
      :lage="lage"
      :thema="thema"
      @thema="setzeThema"
      @angemeldet="angemeldet"
      @erneut="starten"
      @abmelden="abmelden"
    />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--wp-color-bg-canvas);
}

.app__koerper {
  display: flex;
  flex: 1;
  min-height: 0;
}

.app__inhalt {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  /* Höchstens `size.inhalt-max` breit und mittig; der Rand ist mindestens
     `space-5` (Aufgabe 18). */
  padding-inline: max(
    var(--wp-space-5),
    calc((100% - var(--wp-size-inhalt-max)) / 2)
  );
  padding-bottom: var(--wp-space-6);
}
</style>
