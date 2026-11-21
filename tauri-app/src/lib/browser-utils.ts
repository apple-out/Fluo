const TRACKING_QUERY_KEYS = new Set([
  "fbclid",
  "gclid",
  "yclid",
  "mc_cid",
  "mc_eid",
  "igshid",
  "ref_src",
]);

export function normalizeBookmarkUrl(raw: string): string {
  const url = new URL(raw);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("HTTP 또는 HTTPS 페이지만 북마크할 수 있습니다.");
  }
  url.hash = "";
  for (const key of [...url.searchParams.keys()]) {
    const normalized = key.toLowerCase();
    if (normalized.startsWith("utm_") || TRACKING_QUERY_KEYS.has(normalized)) {
      url.searchParams.delete(key);
    }
  }
  return url.toString();
}
