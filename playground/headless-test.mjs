// Headless WASM playground test. Requires Node 18+ and a built
// playground/sukti_wasm.wasm (see scripts/build-wasm.sh).
import fs from "node:fs";
import assert from "node:assert/strict";

const bytes = fs.readFileSync(new URL("./sukti_wasm.wasm", import.meta.url));
const { instance } = await WebAssembly.instantiate(bytes, {});
const ex = instance.exports;
for (const name of [
  "sukti_alloc", "sukti_load", "sukti_step", "sukti_continue", "sukti_reset",
  "sukti_set_breakpoint", "sukti_state", "sukti_state_ptr", "sukti_state_len",
]) {
  assert.equal(typeof ex[name], "function", `missing export ${name}`);
}
assert.ok(ex.memory, "wasm memory must be exported");

function state() {
  ex.sukti_state();
  const len = ex.sukti_state_len();
  const ptr = ex.sukti_state_ptr();
  return JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ptr, len)));
}
function load(src) {
  const b = new TextEncoder().encode(src);
  const p = ex.sukti_alloc(b.length);
  new Uint8Array(ex.memory.buffer, p, b.length).set(b);
  ex.sukti_load(p, b.length);
  return state();
}

// 1. Basic execution.
let s = load("astu sa = 0; sa = 42; vada(sa);");
assert.equal(s.output, "42\n");
assert.equal(s.diagnostics, "");
assert.ok(s.total >= 3, "trace frames recorded");
assert.equal(s.index, 0);
assert.equal(s.tier.mode, "Tier 0");
assert.match(s.tier.note, /not available in a browser/);

// 2. Stepping and reset.
assert.equal(ex.sukti_step(), 1);
assert.equal(state().index, 1);
ex.sukti_reset();
assert.equal(state().index, 0);

// 3. Numeric loop + variables pane.
s = load("astu sa = 0; astu i = 0; yAvat (i < 10) { sa = sa + i; i = i + 1; } vada(sa);");
assert.equal(s.output, "45\n");
ex.sukti_set_breakpoint(3);
s = state();
assert.ok(s.breakpoints.includes(3));
ex.sukti_continue();
s = state();
assert.ok(s.index > 0 && s.index <= s.total);

// 4. Kāraka and sūtra panes.
s = load('kriyA preShaya(kartA ka, karma pa) { phalam ka + ":" + pa; } vada(preShaya(karma: "patra", kartA: "rAma"));');
assert.equal(s.output, "rAma:patra\n");
assert.ok(s.karaka.layer_a >= 1, "Layer A rewrite counted");
s = load("sUtra shulka(va) { phalam 10; }\nvada(shulka(1));");
assert.equal(s.output, "10\n");
assert.ok(s.sutra.length >= 1 && s.sutra[0].includes("shulka"), "sūtra pane populated");

// 5. Diagnostics for a broken program.
s = load("kriyA {");
assert.match(s.diagnostics, /E000/);

console.log("playground WASM headless: all checks passed");
