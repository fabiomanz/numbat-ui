// The UI layer. Everything that knows what a unit is lives in the wasm
// module (crates/web); this file only handles typing, tapping and drawing.
import init, { NumbatSession } from "./pkg/numbat_ui_web.js";

const historyEl = document.getElementById("history");
const placeholderEl = document.getElementById("placeholder");
const inputEl = document.getElementById("input");
const previewEl = document.getElementById("preview");
const completionsEl = document.getElementById("completions");
const keysEl = document.getElementById("keys");
const toastEl = document.getElementById("toast");

let session = null;
/** Position while browsing past inputs with ↑/↓; null = not browsing. */
let navIndex = null;
/** The in-progress line stashed away when browsing started. */
let navStash = "";
let previewTimer = null;

// ---- Startup -------------------------------------------------------------

(async function start() {
  await init();

  // Currency units need exchange rates, and numbat can only be told about
  // them before the first session exists. The file comes from our own origin
  // (the deploy refreshes it daily), so there is no CORS dance and the
  // service worker keeps it available offline.
  session = new NumbatSession(await exchangeRates());
  session.restore();
  render();

  inputEl.disabled = false;
  placeholderEl.textContent = "Type an expression. Results appear as you type.";

  registerServiceWorker();
})();

async function exchangeRates() {
  try {
    const response = await fetch("./exchange-rates.xml");
    // A cold start with no network: currency conversion is simply unavailable.
    return response.ok ? await response.text() : undefined;
  } catch {
    return undefined;
  }
}

// ---- Rendering -----------------------------------------------------------

function render() {
  const entries = JSON.parse(session.entries());
  historyEl.replaceChildren();

  if (entries.length === 0) {
    historyEl.append(placeholderEl);
    return;
  }

  for (const entry of entries) {
    historyEl.append(entryElement(entry));
  }
  scrollToBottom();
}

function entryElement(entry) {
  const card = document.createElement("article");
  card.className = "entry";

  const input = document.createElement("div");
  input.className = "input";
  input.textContent = entry.input;
  card.append(input);

  for (const printed of entry.printed) {
    const line = document.createElement("div");
    line.className = "printed";
    line.innerHTML = printed;
    card.append(line);
  }

  if (entry.result !== null) {
    const result = document.createElement("div");
    result.className = "result";
    result.innerHTML = entry.result;
    result.addEventListener("click", () => {
      copy(entry.resultPlain, card);
    });
    card.append(result);
  }

  if (entry.error !== null) {
    // A phone has the quick panel's problem, not the main window's: lead with
    // the one-line summary and keep the full diagnostic one tap away.
    const error = document.createElement("div");
    error.className = "error";
    error.textContent = entry.error.summary;

    const more = document.createElement("span");
    more.className = "error-more";
    more.textContent = "Show details";
    error.append(more);

    const full = document.createElement("div");
    full.className = "error-full";
    full.textContent = entry.error.full;

    error.addEventListener("click", () => {
      const expanded = card.classList.toggle("expanded");
      more.textContent = expanded ? "Hide details" : "Show details";
    });

    card.append(error, full);
  }

  return card;
}

function scrollToBottom() {
  // After the browser has laid the new cards out.
  requestAnimationFrame(() => {
    historyEl.scrollTop = historyEl.scrollHeight;
  });
}

// ---- Input ---------------------------------------------------------------

inputEl.addEventListener("input", () => {
  navIndex = null;
  schedulePreview();
  updateCompletions();
});

inputEl.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    submit();
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    navigate("up");
  } else if (event.key === "ArrowDown") {
    event.preventDefault();
    navigate("down");
  } else if (event.key === "Tab") {
    event.preventDefault();
    const first = completionsEl.querySelector("button");
    if (first) first.click();
  }
});

function submit() {
  const line = inputEl.value.trim();
  if (line === "") return;

  session.submit(line);
  inputEl.value = "";
  navIndex = null;
  navStash = "";
  setPreview(null);
  updateCompletions();
  render();
}

/** Evaluating on every keystroke is wasteful on a phone; coalesce them. */
function schedulePreview() {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => {
    const raw = session.preview(inputEl.value);
    setPreview(raw === undefined ? null : JSON.parse(raw));
  }, 70);
}

function setPreview(preview) {
  if (!preview) {
    previewEl.hidden = true;
    previewEl.innerHTML = "";
    return;
  }
  previewEl.hidden = false;
  previewEl.innerHTML = preview.html;
  // A value left over from an earlier, still-valid version of the input is
  // dimmed rather than dropped, so it doesn't flicker mid-expression.
  previewEl.classList.toggle("stale", !preview.fresh);
}

// ---- Completions ---------------------------------------------------------

/** The identifier being typed, if the caret sits at the end of one. */
function wordBeforeCaret() {
  const upToCaret = inputEl.value.slice(0, inputEl.selectionStart ?? inputEl.value.length);
  const match = upToCaret.match(/[A-Za-z_][A-Za-z0-9_]*$/);
  return match ? match[0] : "";
}

function updateCompletions() {
  const word = wordBeforeCaret();
  // One letter matches half the unit table; wait for a second.
  if (word.length < 2) {
    completionsEl.hidden = true;
    completionsEl.replaceChildren();
    return;
  }

  const candidates = session.completions(word).slice(0, 12);
  if (candidates.length === 0 || (candidates.length === 1 && candidates[0] === word)) {
    completionsEl.hidden = true;
    completionsEl.replaceChildren();
    return;
  }

  completionsEl.replaceChildren(
    ...candidates.map((candidate) => {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = candidate;
      button.addEventListener("pointerdown", (event) => {
        // Keep the keyboard up: never let the button take focus.
        event.preventDefault();
        completeWith(word, candidate);
      });
      return button;
    })
  );
  completionsEl.hidden = false;
  completionsEl.scrollLeft = 0;
}

function completeWith(word, candidate) {
  const caret = inputEl.selectionStart ?? inputEl.value.length;
  const before = inputEl.value.slice(0, caret - word.length);
  const after = inputEl.value.slice(caret);
  inputEl.value = before + candidate + after;
  const newCaret = before.length + candidate.length;
  inputEl.setSelectionRange(newCaret, newCaret);
  schedulePreview();
  updateCompletions();
}

// ---- Key row -------------------------------------------------------------

keysEl.addEventListener("pointerdown", (event) => {
  const button = event.target.closest("button");
  if (!button) return;
  // Without this the input loses focus and iOS dismisses the keyboard.
  event.preventDefault();

  if (button.dataset.history) {
    navigate(button.dataset.history);
  } else if (button.dataset.insert !== undefined) {
    insert(button.dataset.insert);
  }
});

function insert(text) {
  const start = inputEl.selectionStart ?? inputEl.value.length;
  const end = inputEl.selectionEnd ?? start;
  inputEl.value = inputEl.value.slice(0, start) + text + inputEl.value.slice(end);
  const caret = start + text.length;
  inputEl.setSelectionRange(caret, caret);
  inputEl.focus();
  navIndex = null;
  schedulePreview();
  updateCompletions();
}

// ---- Browsing past inputs ------------------------------------------------

function navigate(direction) {
  const lines = session.history();
  if (lines.length === 0) return;

  if (direction === "up") {
    if (navIndex === null) {
      navStash = inputEl.value;
      navIndex = lines.length - 1;
    } else if (navIndex === 0) {
      return;
    } else {
      navIndex -= 1;
    }
    inputEl.value = lines[navIndex];
  } else {
    if (navIndex === null) return;
    if (navIndex + 1 < lines.length) {
      navIndex += 1;
      inputEl.value = lines[navIndex];
    } else {
      navIndex = null;
      inputEl.value = navStash;
      navStash = "";
    }
  }

  inputEl.focus();
  const end = inputEl.value.length;
  inputEl.setSelectionRange(end, end);
  schedulePreview();
  updateCompletions();
}

// ---- Top bar actions -----------------------------------------------------

document.getElementById("copy-last").addEventListener("click", () => {
  const last = session.last_result();
  if (last === undefined) {
    toast("Nothing to copy");
  } else {
    copy(last);
  }
});

document.getElementById("clear").addEventListener("click", () => {
  // The REPL command, so definitions made so far survive — same as desktop.
  session.submit("clear");
  render();
  inputEl.focus();
});

// ---- Clipboard -----------------------------------------------------------

async function copy(text, card) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    if (!copyFallback(text)) {
      toast("Could not copy");
      return;
    }
  }
  toast(`Copied ${text}`);
  if (card) {
    card.classList.add("copied");
    setTimeout(() => card.classList.remove("copied"), 600);
  }
}

/** For browsers that refuse the async clipboard outside a narrow gesture. */
function copyFallback(text) {
  const scratch = document.createElement("textarea");
  scratch.value = text;
  scratch.setAttribute("readonly", "");
  scratch.style.position = "fixed";
  scratch.style.opacity = "0";
  document.body.append(scratch);
  scratch.select();
  const ok = document.execCommand?.("copy") ?? false;
  scratch.remove();
  return ok;
}

let toastTimer = null;
function toast(message) {
  toastEl.textContent = message;
  toastEl.hidden = false;
  toastEl.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.classList.remove("show");
  }, 1400);
}

// ---- iOS keyboard / viewport ---------------------------------------------

// iOS does not resize the layout viewport for the keyboard, so a bottom-docked
// input would end up underneath it. Track the visual viewport instead.
const viewport = window.visualViewport;
if (viewport) {
  const sync = () => {
    document.body.style.setProperty("--vh", `${viewport.height}px`);
    // Anything more than a sliver of missing height is the keyboard.
    document.body.classList.toggle(
      "keyboard",
      window.innerHeight - viewport.height > 120
    );
    // Safari can leave the page itself scrolled after the keyboard animates.
    window.scrollTo(0, 0);
  };
  viewport.addEventListener("resize", sync);
  viewport.addEventListener("scroll", sync);
  sync();
}

inputEl.addEventListener("focus", () => {
  // The newest entry should stay in view once the keyboard is up.
  setTimeout(scrollToBottom, 300);
});

// ---- Service worker ------------------------------------------------------

function registerServiceWorker() {
  if (!("serviceWorker" in navigator)) return;
  navigator.serviceWorker.register("./sw.js").catch(() => {
    // Offline support is a bonus; the app works without it.
  });
}
