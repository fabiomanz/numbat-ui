// Offline support. The cache name carries the build stamp that
// scripts/build-web.sh substitutes, so every deploy installs a fresh cache
// and drops the old one — the wasm binary and its JS glue can never end up
// on different versions.
const CACHE = "numbat-__CACHE_VERSION__";

const SHELL = [
  "./",
  "./index.html",
  "./style.css",
  "./app.js",
  "./manifest.webmanifest",
  "./exchange-rates.xml",
  "./pkg/numbat_ui_web.js",
  "./pkg/numbat_ui_web_bg.wasm",
  "./icons/logo.png",
  "./icons/icon-192.png",
  "./icons/icon-512.png",
  "./icons/apple-touch-icon.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      // `reload` bypasses the HTTP cache, so a deploy is never precached stale.
      .then((cache) => cache.addAll(SHELL.map((url) => new Request(url, { cache: "reload" }))))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(names.filter((n) => n !== CACHE).map((n) => caches.delete(n))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET" || new URL(request.url).origin !== self.location.origin) {
    return;
  }

  // Everything in this cache belongs to this exact build, so serving from it
  // is both correct and instant; the network is only a fallback.
  event.respondWith(
    caches.match(request).then(
      (hit) =>
        hit ??
        fetch(request).catch(() => {
          // A navigation that missed the cache while offline still gets the app.
          if (request.mode === "navigate") return caches.match("./index.html");
          throw new Error("offline");
        })
    )
  );
});
