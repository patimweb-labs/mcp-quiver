// Tiefe Kopie für die Bearbeitung. `structuredClone` scheitert an den
// reaktiven Proxys von Vue; die Modelle sind reines JSON, also ist der Weg
// über JSON hier genau und billig.

/** Kopie, die niemanden mehr mit dem Original teilt. */
export function tief_kopie<T>(wert: T): T {
  return JSON.parse(JSON.stringify(wert)) as T;
}
