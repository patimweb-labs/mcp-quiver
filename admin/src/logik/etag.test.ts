import { describe, expect, it } from "vitest";

import { etag, fnv1a, kanonisch } from "./etag";

// Die festen Werte stammen aus derselben Rechnung im Server
// (`crates/server/src/hilfen.rs`): beide Seiten müssen dasselbe ETag bilden.
describe("ETag", () => {
  it("hängt nicht an der Reihenfolge der Schlüssel", () => {
    expect(kanonisch({ b: 1, a: [1, 2] })).toBe('{"a":[1,2],"b":1}');
    expect(etag({ b: 1, a: [1, 2] })).toBe(etag({ a: [1, 2], b: 1 }));
  });

  it("bildet die festen Werte des Servers", () => {
    expect(etag({ b: 1, a: [1, 2] })).toBe('"30077cb0d8fc07be"');
    expect(etag({})).toBe('"08f44b07b5901a25"');
    expect(etag([])).toBe('"09612b07b5ecb5a5"');
  });

  it("ändert sich mit dem Inhalt", () => {
    expect(etag({ a: [1, 3], b: 1 })).not.toBe(etag({ a: [1, 2], b: 1 }));
  });

  it("FNV-1a: leerer Text ist der Startwert", () => {
    expect(fnv1a("").toString(16)).toBe("cbf29ce484222325");
  });
});
