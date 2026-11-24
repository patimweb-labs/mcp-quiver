<script setup lang="ts">
// Anmelden (Aufgabe 20 und 25). Zwei Wege: Root mit E-Mail und Passwort, und –
// nur wenn der Server Entra kennt – „Mit Microsoft anmelden“.
//
// Kein Token, kein Feld für einen Schlüssel, nichts im Speicher des Browsers:
// die Sitzung liegt im HttpOnly-Cookie, das der Server setzt. Das Formular
// trägt `autocomplete`-Namen, damit der Passwortmanager es erkennt.
import { computed, reactive, ref } from "vue";
import { WpButton, WpInput } from "@weppelmann/components";

import { ApiFehler, MICROSOFT_ANMELDUNG, anmelden, fehlertext } from "../api/client";
import type { Ich } from "../api/typen";
import { anmeldung_fehler, email_fehler } from "../logik/anmeldung";
import Feld from "../bausteine/Feld.vue";
import Karte from "../bausteine/Karte.vue";
import Verweis from "../bausteine/Verweis.vue";

const props = defineProps<{
  /** Kennt der Server Entra? Nur dann gibt es den Microsoft-Weg. */
  entra: boolean;
  /** Warum diese Seite erscheint, z. B. „Die Anmeldung ist abgelaufen.“ */
  hinweis?: string;
}>();
const emit = defineEmits<{ angemeldet: [ich: Ich] }>();

const email = ref("");
const passwort = ref("");
const laedt = ref(false);
const eigenerFehler = ref("");
const beruehrt = reactive({ email: false, passwort: false });

// Erst nach dem Verlassen des Felds: beim Tippen wäre jeder Anfang ein Fehler.
const emailFehler = computed<string | undefined>(() =>
  beruehrt.email ? (email_fehler(email.value) ?? undefined) : undefined,
);

const angezeigt = computed(() => eigenerFehler.value || props.hinweis || "");

async function senden(): Promise<void> {
  eigenerFehler.value = "";
  beruehrt.email = true;
  beruehrt.passwort = true;
  const fehler = anmeldung_fehler(email.value, passwort.value);
  if (fehler) {
    eigenerFehler.value = fehler.text;
    return;
  }
  laedt.value = true;
  try {
    emit("angemeldet", await anmelden(email.value.trim(), passwort.value));
  } catch (wert) {
    eigenerFehler.value =
      wert instanceof ApiFehler && wert.status === 401
        ? "E-Mail oder Passwort stimmt nicht."
        : fehlertext(wert);
  } finally {
    laedt.value = false;
  }
}
</script>

<template>
  <Karte
    titel="Anmelden"
    titel_als="h1"
    unter="Mit dem Root-Konto oder dem Microsoft-Konto."
  >
    <form class="form" novalidate @submit.prevent="senden">
      <Feld beschriftung="E-Mail" :fehler="emailFehler">
        <template #default="{ id }">
          <WpInput
            :id="id"
            v-model="email"
            type="text"
            placeholder="du@example.test"
            autocomplete="username"
            autocapitalize="none"
            spellcheck="false"
            :state="emailFehler ? 'invalid' : 'default'"
            @blur="beruehrt.email = true"
            @keyup.enter="senden"
          />
        </template>
      </Feld>

      <Feld beschriftung="Passwort">
        <template #default="{ id }">
          <WpInput
            :id="id"
            v-model="passwort"
            type="password"
            autocomplete="current-password"
            @keyup.enter="senden"
          />
        </template>
      </Feld>

      <p v-if="angezeigt" class="form__fehler" role="alert">{{ angezeigt }}</p>

      <div class="form__aktion">
        <WpButton
          label="Anmelden"
          variant="primary"
          :state="laedt ? 'busy' : 'default'"
          @press="senden"
        />
      </div>
    </form>

    <!-- Nur wenn der Server Entra kennt; vorher wäre der Knopf ein Versprechen
         ohne Deckung. Der Weg gehört dem Browser: er folgt der Weiterleitung
         zu Entra und kommt mit dem Sitzungscookie zurück. -->
    <div v-if="entra" class="wechsel">
      <p class="wechsel__text">oder</p>
      <Verweis :href="MICROSOFT_ANMELDUNG" label="Mit Microsoft anmelden" />
    </div>
  </Karte>
</template>

<style scoped>
.form {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.form__fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}

.form__aktion {
  display: flex;
  justify-content: flex-end;
  gap: var(--wp-space-2);
  padding-top: var(--wp-space-2);
}

.wechsel {
  display: flex;
  align-items: center;
  gap: var(--wp-space-3);
  padding-top: var(--wp-space-4);
  border-top: var(--wp-size-border) solid var(--wp-color-border-subtle);
  margin-top: var(--wp-space-4);
}

.wechsel__text {
  color: var(--wp-color-text-faint);
  font-size: var(--wp-font-size-sm);
}
</style>
