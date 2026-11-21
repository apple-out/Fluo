# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in **Fluo**, please report it privately via GitHub Security Advisories instead of opening a public issue:

- **Private Security Advisory:** https://github.com/apple-out/transratebrowser/security/advisories/new

Please include reproduction steps, affected version/commit, and potential impact. We will review and respond via the advisory thread.

## Local Credential & Data Storage Architecture

- **Secrets at rest:** API keys, local bridge tokens, OAuth session tokens, and saved website logins are encrypted using Windows **DPAPI (`CryptProtectData`)** bound to the current OS user profile (`secrets.dpapi.json`).
- **Sync boundaries:** Optional profile sync (`sync-profile.json` in your selected cloud drive or local sync folder) explicitly excludes API keys, OAuth tokens, saved passwords, and private-browsing tabs.
- **Diagnostics:** Diagnostic reports automatically mask bearer tokens, API keys, local usernames, and Windows filesystem paths (`C:\Users\<user>\...`).
