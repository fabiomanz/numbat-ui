// Exercises the wasm boundary from JavaScript: the Rust side is covered by
// `cargo test` in numbat-ui-core, but the shapes that cross into JS — the
// entry JSON, the markup HTML, the one-shot exchange-rate handover — are only
// visible from here.
//
// Run it with scripts/test-web.sh (which builds node-targeted bindings first).
const path = require("node:path");
const fs = require("node:fs");

const { NumbatSession } = require(path.resolve(process.argv[2], "numbat_ui_web.js"));
const ratesPath = process.argv[3];
const rates = ratesPath && fs.existsSync(ratesPath) ? fs.readFileSync(ratesPath, "utf8") : "";

let failures = 0;
function check(label, actual, expected) {
  const ok = Object.is(actual, expected);
  if (!ok) failures++;
  const detail = ok ? "" : `  (expected ${JSON.stringify(expected)})`;
  console.log(`${ok ? "ok  " : "FAIL"}  ${label}: ${JSON.stringify(actual)}${detail}`);
}

const s = new NumbatSession(rates || undefined);

// ---- Live preview: shows a value without committing anything --------------
const preview = JSON.parse(s.preview("30 km/h -> mph"));
check("preview evaluates", preview.plain, "18.6411 mph");
check("preview is fresh", preview.fresh, true);
check("preview does not commit", JSON.parse(s.entries()).length, 0);
// Mid-expression the last good value sticks around instead of flickering out.
check("stale preview sticks", JSON.parse(s.preview("30 km/h ->")).fresh, false);
check("preview of nothing", s.preview(""), undefined);

// ---- Submitting, definitions, history -------------------------------------
s.submit("let r = 3 m");
s.submit("2 * r");
const entries = JSON.parse(s.entries());
check("entry count", entries.length, 2);
check("definitions carry over", entries[1].resultPlain, "6 m");
check("plain result for the clipboard", s.last_result(), "6 m");
check("history is the submitted lines", s.history().join("|"), "let r = 3 m|2 * r");
check("markup becomes classed html", entries[1].result.includes('class="nb-value"'), true);

// ---- Untrusted text may never reach the DOM as markup ---------------------
s.submit('"<img src=x onerror=alert(1)>"');
const escaped = JSON.parse(s.entries())[2].result;
check("html is escaped", escaped.includes("&lt;img") && !escaped.includes("<img"), true);

// ---- Errors arrive in both renderings -------------------------------------
s.submit("accos(1/2)");
const error = JSON.parse(s.entries())[3].error;
check("full diagnostic has the caret", error.full.includes("^"), true);
check("summary is one line", error.summary.includes("\n"), false);
check("summary carries the hint", error.summary.includes("Did you mean 'acos'?"), true);

// ---- Completions ----------------------------------------------------------
check("completions", s.completions("sqr").some((c) => c.startsWith("sqrt")), true);
check("no completions for nothing", s.completions("").length, 0);

// ---- REPL commands --------------------------------------------------------
s.submit("clear");
check("clear empties the view", JSON.parse(s.entries()).length, 0);
s.submit("r");
check("clear keeps definitions", JSON.parse(s.entries())[0].resultPlain, "3 m");
s.submit("reset");
s.submit("r");
check("reset drops definitions", JSON.parse(s.entries())[0].error !== null, true);
s.submit("help");
check("help renders", JSON.parse(s.entries())[1].printed.length > 0, true);

// ---- Degrading without a browser ------------------------------------------
// There is no localStorage in node; the store must shrug rather than throw.
s.restore();
check("survives without localStorage", true, true);

// ---- Formatting knobs -----------------------------------------------------
// A second session must not trip over numbat's one-shot exchange-rate cache.
const formatted = new NumbatSession(rates || undefined);
formatted.set_formatting(",", 4, 3);
formatted.submit("1234567 * 1.0");
check("digit separator applies", JSON.parse(formatted.entries())[0].resultPlain, "1,234,567");

// ---- Currency needs the rates we were handed ------------------------------
if (rates) {
  const fx = new NumbatSession(rates);
  fx.submit("100 EUR -> USD");
  const entry = JSON.parse(fx.entries())[0];
  check("currency converts", entry.error === null && entry.resultPlain !== null, true);
} else {
  console.log("skip  currency conversion (no exchange rates available)");
}

// A session with no rates at all still has to work.
const bare = new NumbatSession(undefined);
bare.submit("2 + 2");
check("works without exchange rates", JSON.parse(bare.entries())[0].resultPlain, "4");

console.log(failures === 0 ? "\nAll checks passed." : `\n${failures} check(s) failed.`);
process.exit(failures === 0 ? 0 : 1);
