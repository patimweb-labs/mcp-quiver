<script setup lang="ts">
// Die Ersteinrichtung (Aufgabe 25): Es gibt noch keinen Root, und nur dieses
// Zeitfenster (15 Minuten nach dem Start des Servers) kann einen anlegen.
// Danach meldet der Server den neuen Root gleich an; diese Seite kommt nie
// wieder.
//
// Mindestlänge und Gleichheit des Passworts sind sofort sichtbar, nicht erst
// nach dem Absenden. Der Code steht nur da, wenn der Server ihn verlangt
// (`code_noetig`: der Server ist nicht auf Loopback).
import { computed, reactive, ref } from "vue";
import { Eye, EyeOff } from "lucide-vue-next";
import { WpButton, WpIconButton, WpInput } from "@weppelmann/components";

import { ApiFehler, einrichten, fehlertext } from "../api/client";
import type { Ich } from "../api/typen";
import {
  PASSWORT_MIN,
  einrichtung_fehler,
  email_fehler,
  passwort_fehler,
  wiederholung_fehler,
} from "../logik/anmeldung";
import Feld from "../bausteine/Feld.vue";
import Karte from "../bausteine/Karte.vue";

const props = defineProps<{ code_noetig: boolean }>();
const emit = defineEmits<{ angemeldet: [ich: Ich] }>();

const felder = reactive({ email: "", passwort: "", wiederholung: "", code: "" });
const beruehrt = reactive({ email: false });
const sichtbar = ref(false);
const laedt = ref(false);
const meldung = ref("");

// Sofort sichtbar: die Mindestlänge, sobald etwas im Feld steht, und die
// Gleichheit, sobald die Wiederholung anfängt. Beim leeren Feld schweigen
// beide – ein Fehler vor dem ersten Buchstaben ist keiner.
const passwortFehler = computed<string | undefined>(() =>
  felder.passwort.length > 0
    ? (passwort_fehler(felder.passwort) ?? undefined)
    : undefined,
);
const wiederholungFehler = computed<string | undefined>(() =>
  felder.wiederholung.length > 0
    ? (wiederholung_fehler(felder.passwort, felder.wiederholung) ?? undefined)
    : undefined,
);
const emailFehler = computed<string | undefined>(() =>
  beruehrt.email ? (email_fehler(felder.email) ?? undefined) : undefined,
);

async function senden(): Promise<void> {
  meldung.value = "";
  beruehrt.email = true;
  const fehler = einrichtung_fehler(felder, props.code_noetig);
  if (fehler) {
    meldung.value = fehler.text;
    return;
  }
  laedt.value = true;
  try {
    const code = props.code_noetig ? felder.code.trim() : "";
    emit(
      "angemeldet",
      await einrichten({
        email: felder.email.trim(),
        passwort: felder.passwort,
        ...(code ? { code } : {}),
      }),
    );
  } catch (wert) {
    meldung.value = eingerichtet_oder_fehler(wert);
  } finally {
    laedt.value = false;
  }
}

function eingerichtet_oder_fehler(wert: unknown): string {
  if (wert instanceof ApiFehler && wert.status === 404) {
    return "Dieser Server ist schon eingerichtet. Bitte anmelden.";
  }
  return fehlertext(wert);
}
</script>

<template>
  <div class="einrichtung">
    <header class="einrichtung__kopf">
      <h1 class="einrichtung__titel">Willkommen bei mcp-quiver</h1>
      <p class="einrichtung__text">
        Dieser Server hat noch keinen Root-Benutzer. Lege ihn jetzt an – er ist
        der Notzugang, wenn Microsoft nicht erreichbar ist. Es gibt genau einen.
      </p>
    </header>

    <Karte
      titel="Root anlegen"
      unter="Danach bist du angemeldet und diese Seite verschwindet."
    >
      <form class="einrichtung__form" novalidate @submit.prevent="senden">
        <Feld beschriftung="E-Mail" :fehler="emailFehler">
          <template #default="{ id }">
            <WpInput
              :id="id"
              v-model="felder.email"
              type="text"
              placeholder="du@example.test"
              autocomplete="username"
              autocapitalize="none"
              spellcheck="false"
              :state="emailFehler ? 'invalid' : 'default'"
              @blur="beruehrt.email = true"
            />
          </template>
        </Feld>

        <Feld
          beschriftung="Passwort"
          :hinweis="`Mindestens ${PASSWORT_MIN} Zeichen.`"
          :fehler="passwortFehler"
        >
          <template #default="{ id }">
            <div class="einrichtung__passwort">
              <WpInput
                :id="id"
                v-model="felder.passwort"
                :type="sichtbar ? 'text' : 'password'"
                autocomplete="new-password"
                :state="passwortFehler ? 'invalid' : 'default'"
              />
              <WpIconButton
                :icon="sichtbar ? EyeOff : Eye"
                :label="sichtbar ? 'Passwort verbergen' : 'Passwort anzeigen'"
                :active="sichtbar"
                variant="ghost"
                size="sm"
                @press="sichtbar = !sichtbar"
              />
            </div>
          </template>
        </Feld>

        <Feld
          beschriftung="Passwort wiederholen"
          :fehler="wiederholungFehler"
        >
          <template #default="{ id }">
            <WpInput
              :id="id"
              v-model="felder.wiederholung"
              :type="sichtbar ? 'text' : 'password'"
              autocomplete="new-password"
              :state="wiederholungFehler ? 'invalid' : 'default'"
            />
          </template>
        </Feld>

        <!-- Nur wenn der Server nicht auf Loopback lauscht: dann beweist der
             Einmal-Code, dass wer hier sitzt an den Server kommt. -->
        <Feld
          v-if="code_noetig"
          beschriftung="Einmal-Code"
          hinweis="Steht in der Log-Ausgabe des Servers (im Container: docker logs <container>, mit Compose: docker compose logs quiver). Er wird bei jedem Start neu erzeugt."
        >
          <template #default="{ id }">
            <WpInput
              :id="id"
              v-model="felder.code"
              type="text"
              placeholder="Code aus dem Server-Log"
              autocomplete="one-time-code"
              autocapitalize="none"
              spellcheck="false"
            />
          </template>
        </Feld>

        <p v-if="meldung" class="einrichtung__fehler" role="alert">
          {{ meldung }}
        </p>

        <div class="einrichtung__aktion">
          <WpButton
            label="Root anlegen und anmelden"
            variant="primary"
            :state="laedt ? 'busy' : 'default'"
            @press="senden"
          />
        </div>
      </form>
    </Karte>
  </div>
</template>

<style scoped>
.einrichtung {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-4);
}

.einrichtung__kopf {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.einrichtung__titel {
  font-size: var(--wp-font-size-2xl);
  font-weight: var(--wp-font-weight-semibold);
  line-height: var(--wp-font-line-tight);
}

.einrichtung__text {
  color: var(--wp-color-text-muted);
  max-width: calc(var(--wp-space-8) * 9);
}

.einrichtung__form {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.einrichtung__passwort {
  display: flex;
  align-items: center;
  gap: var(--wp-space-2);
}

.einrichtung__fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}

.einrichtung__aktion {
  display: flex;
  justify-content: flex-end;
  padding-top: var(--wp-space-2);
}
</style>
