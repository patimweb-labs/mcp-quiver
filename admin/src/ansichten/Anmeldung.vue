<script setup lang="ts">
// Anmeldung mit dem Admin-Token. Der Token steht nur in `sessionStorage` und
// wird vor dem Speichern geprüft; der Server vergleicht in konstanter Zeit.
import { computed, ref } from "vue";
import { WpButton, WpInput } from "@weppelmann/components";

import { ApiFehler, Client, fehlertext } from "../api/client";
import type { Thema } from "../api/typen";
import Feld from "../bausteine/Feld.vue";
import Karte from "../bausteine/Karte.vue";
import Marke from "../bausteine/Marke.vue";

const props = defineProps<{ fehler?: string; thema: Thema }>();
const emit = defineEmits<{ angemeldet: [string] }>();

const token = ref("");
const laedt = ref(false);
const eigenerFehler = ref("");

const angezeigt = computed(() => eigenerFehler.value || props.fehler || "");

async function anmelden(): Promise<void> {
  const wert = token.value.trim();
  eigenerFehler.value = "";
  if (wert.length === 0) {
    eigenerFehler.value = "Bitte den Admin-Token eingeben.";
    return;
  }
  laedt.value = true;
  try {
    await new Client(wert).ueberblick();
    emit("angemeldet", wert);
  } catch (fehler) {
    eigenerFehler.value =
      fehler instanceof ApiFehler && fehler.status === 401
        ? "Dieser Token passt nicht."
        : fehlertext(fehler);
  } finally {
    laedt.value = false;
  }
}
</script>

<template>
  <main class="anmeldung">
    <div class="anmeldung__kopf">
      <Marke :thema="thema" />
      <p class="anmeldung__unter">Verwaltung</p>
    </div>
    <div class="anmeldung__breite">
      <Karte
        titel="Anmelden"
        unter="Der Admin-Token steht auf dem Server in daten/admin-token."
      >
        <form class="anmeldung__form" @submit.prevent="anmelden">
          <Feld beschriftung="Admin-Token" hinweis="Wird nur für diese Sitzung gemerkt.">
            <template #default="{ id }">
              <WpInput
                :id="id"
                v-model="token"
                type="password"
                placeholder="Token"
                :state="angezeigt ? 'invalid' : 'default'"
                autocomplete="off"
                @keyup.enter="anmelden"
              />
            </template>
          </Feld>
          <p v-if="angezeigt" class="anmeldung__fehler" role="alert">
            {{ angezeigt }}
          </p>
          <div class="anmeldung__aktion">
            <WpButton
              label="Anmelden"
              variant="primary"
              :state="laedt ? 'busy' : 'default'"
              @press="anmelden"
            />
          </div>
        </form>
      </Karte>
    </div>
  </main>
</template>

<style scoped>
.anmeldung {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  gap: var(--wp-space-5);
  min-height: 100%;
  padding: var(--wp-space-7) var(--wp-space-4);
  overflow-y: auto;
}

.anmeldung__kopf {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--wp-space-1);
}

.anmeldung__unter {
  color: var(--wp-color-text-muted);
  font-size: var(--wp-font-size-sm);
}

.anmeldung__breite {
  width: min(calc(var(--wp-space-8) * 8), 100%);
}

.anmeldung__form {
  display: flex;
  flex-direction: column;
  gap: var(--wp-space-2);
}

.anmeldung__fehler {
  color: var(--wp-color-status-error);
  font-size: var(--wp-font-size-sm);
}

.anmeldung__aktion {
  display: flex;
  justify-content: flex-end;
  padding-top: var(--wp-space-2);
}
</style>
