# Open-Beta Feedback & Anonymous Relay Setup Guide

Fluo uses a **privacy-first, unlimited feedback system** that never embeds or exposes any maintainer email address.

## 1. Default Built-In Flow (Zero Setup, Unlimited)

By default, testers can submit bug reports, UX feedback, and translation quality reports inside the browser via:

**Menu (`⋯`) → Report Bug / Translation Error** (or Settings → **Data · Feedback**)

Without any relay server configured:
1. The app formats a structured Markdown report with sanitized diagnostics (OS, WebView2 runtime, GPU/CPU backend, installed GGUF filename — never API keys, passwords, cookies, or browsing history).
2. Clicking **Create GitHub Issue** opens the pre-filled GitHub Issue form on `https://github.com/apple-out/transratebrowser/issues/new` with **no daily quota**.
3. Testers can also copy the full Markdown report or export it as a `.md` file.

---

## 2. Optional: Anonymous Zero-Login Relay (Cloudflare Workers — 100,000 req/day Free)

If you want testers to submit feedback **without logging into GitHub**, deploy [`cloudflare-worker-feedback.js`](./cloudflare-worker-feedback.js) on Cloudflare Workers:

### Why Cloudflare Workers instead of Google Apps Script Email?
- **No 100/day email quota** (Cloudflare Workers Free tier provides 100,000 requests/day).
- **Zero email exposure** (issues go straight to your private triage webhook or public GitHub Issues).
- **Zero maintainer inbox spam**.

### Setup Steps
1. Sign in to [Cloudflare Dashboard](https://dash.cloudflare.com/) → **Workers & Pages** → **Create Worker**.
2. Paste the contents of [`scripts/cloudflare-worker-feedback.js`](./cloudflare-worker-feedback.js) and deploy.
   Your worker URL will look like: `https://tb-feedback.<your-subdomain>.workers.dev`
3. In Cloudflare Worker **Settings → Variables & Secrets**, add:
   - `GITHUB_TOKEN` *(Secret)*: A GitHub Fine-grained Personal Access Token scoped only to `apple-out/transratebrowser` with **Issues: Read and write**.
   - `GITHUB_REPO` *(Variable, optional)*: Defaults to `apple-out/transratebrowser`.
   - `WEBHOOK_URL` *(Secret, optional)*: Discord/Slack/ntfy incoming webhook URL for real-time notifications.
4. Distribute the Worker URL to the app using **any one** of these methods:
   - Enter it in **Settings → Data · Feedback → Relay URL**, OR
   - Place a single-line `feedback-endpoint.txt` file next to `Fluo.exe`, OR
   - Set the build-time environment variable `TB_FEEDBACK_WEBHOOK_URL=https://tb-feedback.<your-subdomain>.workers.dev` before compiling release binaries.

Supported hosted form relays:
- `https://*.workers.dev/*` (Cloudflare Workers)
- `https://formspree.io/f/*`
- `https://api.web3forms.com/submit`

> Legacy `script.google.com` Apps Script endpoints are rejected intentionally to eliminate the 100-email daily limit and avoid leaking Gmail addresses.
