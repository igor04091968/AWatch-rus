self.addEventListener("install", function (event) {
  self.skipWaiting();
  event.waitUntil((async function () {
    const keys = await caches.keys();
    await Promise.all(keys.map(function (key) { return caches.delete(key); }));
  })());
});

self.addEventListener("activate", function (event) {
  event.waitUntil((async function () {
    const keys = await caches.keys();
    await Promise.all(keys.map(function (key) { return caches.delete(key); }));
    await self.clients.claim();
    await self.registration.unregister();
  })());
});

self.addEventListener("fetch", function () {});
