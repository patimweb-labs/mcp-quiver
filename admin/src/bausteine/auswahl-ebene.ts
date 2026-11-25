// Die gemeinsame Mechanik aller Auswahllisten der Verwaltung (Aufgabe 33).
//
// Eine Liste hängt am Feld, liegt aber **nicht** im Dialog: sie wird an den
// `body` gebeamt und in einer eigenen Ebene über allem gezeigt. So wächst der
// Dialog nicht und bekommt keinen Scrollbalken, und die Liste kann höchstens so
// hoch werden wie der freie Platz. Beim Scrollen und bei Größenänderung folgt
// sie dem Feld oder schließt sich.
//
// Die Rechnung steht in `logik/liste.ts`; hier kommt nur das DOM dazu.

import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";

// Die Ebene ist für beide Bausteine dieselbe; ihr Aussehen lädt mit ihnen.
import "./auswahl-ebene.css";

import { anker_sichtbar, listenplatz, type Listenplatz } from "../logik/liste";

let zaehler = 0;

/** Eine eindeutige Kennung für ARIA-Verknüpfungen (`aria-controls`). */
export function naechste_kennung(art: string): string {
  zaehler += 1;
  return `${art}-${zaehler}`;
}

/**
 * Öffnen, Schließen, Platz messen: alles, was Auswahl und Mehrfachauswahl
 * gemeinsam haben.
 */
export function useAuswahlEbene() {
  const offen = ref(false);
  const anker = ref<HTMLElement | null>(null);
  const ebene = ref<HTMLElement | null>(null);
  const platz = ref<Listenplatz | null>(null);

  // Template-Refs aus Vue heraus setzen (die Bausteine hängen sie an `:ref`).
  function anker_setzen(el: unknown): void {
    anker.value = (el as HTMLElement | null) ?? null;
  }
  function ebene_setzen(el: unknown): void {
    ebene.value = (el as HTMLElement | null) ?? null;
  }

  function messen(): void {
    const feld = anker.value;
    if (!feld) return;
    const masse = feld.getBoundingClientRect();
    const fenster = { breite: window.innerWidth, hoehe: window.innerHeight };
    const anker_mass = {
      oben: masse.top,
      unten: masse.bottom,
      links: masse.left,
      breite: masse.width,
    };
    if (!anker_sichtbar(anker_mass, fenster)) {
      schliessen();
      return;
    }
    platz.value = listenplatz(anker_mass, fenster);
  }

  function oeffnen(): void {
    if (offen.value) return;
    // Erst messen, wenn das Feld steht: sonst ist die Höhe 0.
    platz.value = null;
    offen.value = true;
    void nextTick(messen);
  }

  function schliessen(): void {
    offen.value = false;
    platz.value = null;
  }

  function umschalten(): void {
    if (offen.value) schliessen();
    else oeffnen();
  }

  /** Scrollen außerhalb der Liste bewegt das Feld – die Liste folgt. */
  function bei_scrollen(event: Event): void {
    if (!offen.value) return;
    if (ebene.value && event.target instanceof Node && ebene.value.contains(event.target)) {
      return;
    }
    messen();
  }

  /** Klick daneben schließt; ein Klick im Feld oder in der Liste nicht. */
  function bei_zeiger(event: MouseEvent): void {
    if (!offen.value) return;
    const ziel = event.target as Node | null;
    if (ziel && (anker.value?.contains(ziel) || ebene.value?.contains(ziel))) return;
    schliessen();
  }

  onMounted(() => {
    document.addEventListener("scroll", bei_scrollen, true);
    document.addEventListener("mousedown", bei_zeiger, true);
    window.addEventListener("resize", messen);
  });
  onBeforeUnmount(() => {
    document.removeEventListener("scroll", bei_scrollen, true);
    document.removeEventListener("mousedown", bei_zeiger, true);
    window.removeEventListener("resize", messen);
  });

  /** Die Maße der Ebene in Pixeln; leer, solange nicht gemessen wurde. */
  const stil = computed(() => {
    const masse = platz.value;
    if (!masse) return {};
    return {
      top: `${masse.oben}px`,
      left: `${masse.links}px`,
      width: `${masse.breite}px`,
      maxHeight: `${masse.hoehe}px`,
    };
  });

  return {
    offen,
    platz,
    stil,
    anker_setzen,
    ebene_setzen,
    oeffnen,
    schliessen,
    umschalten,
    messen,
  };
}
