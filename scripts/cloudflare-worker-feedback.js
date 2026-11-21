/**
 * Cloudflare Worker — Fluo Unlimited Anonymous Feedback Relay
 * ----------------------------------------------------------
 * Replaces daily-capped email scripts (100/day limit) with Cloudflare Workers
 * (100,000 requests/day on the Free plan) and creates GitHub Issues without
 * exposing any developer email address or requiring testers to sign in.
 *
 * Environment Variables / Secrets (set in Cloudflare Dashboard or wrangler.toml):
 * - GITHUB_TOKEN (Secret): Fine-grained PAT with Issues: Read and write on target repo
 * - GITHUB_REPO  (Var, optional): "apple-out/transratebrowser" (default)
 * - WEBHOOK_URL  (Secret, optional): Slack/Discord/ntfy webhook for instant alerts
 * - ALLOWED_APP  (Var, optional): Shared header/origin filter if desired
 */

const DEFAULT_REPO = "apple-out/transratebrowser";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

function jsonResponse(payload, status = 200) {
  return new Response(JSON.stringify(payload), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      ...CORS_HEADERS,
    },
  });
}

function redactSensitiveText(input) {
  if (!input || typeof input !== "string") return "";
  return input
    .replace(/AIza[0-9A-Za-z_-]{20,}/g, "[REDACTED_API_KEY]")
    .replace(/sk-[0-9A-Za-z_-]{16,}/g, "[REDACTED_KEY]")
    .replace(/Bearer\s+[0-9A-Za-z._-]{16,}/gi, "Bearer [REDACTED]")
    .slice(0, 12000);
}

function buildIssueTitle(data) {
  const explicitTitle = String(data.title || "").trim();
  if (explicitTitle) return explicitTitle.slice(0, 180);

  const category = String(data.category || "bug").toLowerCase();
  const tag =
    category === "translation"
      ? "[Translation]"
      : category === "ux"
        ? "[UX]"
        : "[Bug]";
  const summary = String(data.summary || "Open Beta Feedback").trim();
  return `${tag} ${summary}`.slice(0, 180);
}

function buildIssueLabels(category) {
  const c = String(category || "bug").toLowerCase();
  if (c === "translation") return ["open-beta", "translation-quality"];
  if (c === "ux") return ["open-beta", "enhancement"];
  return ["open-beta", "bug"];
}

function buildIssueBody(data) {
  const explicitBody = String(data.body || "").trim();
  if (explicitBody) {
    return redactSensitiveText(explicitBody);
  }

  const lines = [];
  const category = String(data.category || "bug");
  const version = String(data.appVersion || data.app_version || "unknown");
  const description = redactSensitiveText(String(data.description || "").trim());
  const pageUrl = redactSensitiveText(String(data.pageUrl || data.page_url || "").trim());
  const sourceText = redactSensitiveText(String(data.sourceText || data.source_text || "").trim());
  const translatedText = redactSensitiveText(String(data.translatedText || data.translated_text || "").trim());
  const suggested = redactSensitiveText(String(data.suggestedTranslation || data.suggested_translation || "").trim());
  const contact = redactSensitiveText(String(data.contact || "").trim());
  const diagnostics = redactSensitiveText(String(data.diagnostics || "").trim());

  lines.push(`**Category:** \`${category}\` | **Version:** \`${version}\``);
  lines.push("");
  if (description) {
    lines.push("### Description");
    lines.push(description);
    lines.push("");
  }
  if (pageUrl) {
    lines.push(`**Page URL:** ${pageUrl}`);
    lines.push("");
  }
  if (sourceText || translatedText || suggested) {
    lines.push("### Translation Context");
    if (sourceText) lines.push(`- **Source:**\n> ${sourceText.replace(/\n/g, "\n> ")}`);
    if (translatedText) lines.push(`- **Output:**\n> ${translatedText.replace(/\n/g, "\n> ")}`);
    if (suggested) lines.push(`- **Suggested:**\n> ${suggested.replace(/\n/g, "\n> ")}`);
    lines.push("");
  }
  if (contact) {
    lines.push(`**Reporter Contact:** ${contact}`);
    lines.push("");
  }
  if (diagnostics) {
    lines.push("<details><summary>Sanitized Diagnostics</summary>\n\n```text\n" + diagnostics + "\n```\n</details>");
  }
  return lines.join("\n").trim() || "No details provided.";
}

export default {
  async fetch(request, env) {
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }
    if (request.method !== "POST") {
      return jsonResponse({ ok: false, error: "Method Not Allowed" }, 405);
    }

    let data;
    try {
      data = await request.json();
    } catch {
      return jsonResponse({ ok: false, error: "Invalid JSON payload" }, 400);
    }

    const title = buildIssueTitle(data);
    const body = buildIssueBody(data);
    if (!title && !body) {
      return jsonResponse({ ok: false, error: "Empty feedback payload" }, 400);
    }

    const repo = String(env.GITHUB_REPO || DEFAULT_REPO).trim();
    const githubToken = String(env.GITHUB_TOKEN || "").trim();
    let issueUrl = "";
    let issueNumber = null;

    if (githubToken) {
      const ghRes = await fetch(`https://api.github.com/repos/${repo}/issues`, {
        method: "POST",
        headers: {
          "Accept": "application/vnd.github+json",
          "Authorization": `Bearer ${githubToken}`,
          "User-Agent": "Fluo-Feedback-Relay/1.0",
          "X-GitHub-Api-Version": "2022-11-28",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          title,
          body,
          labels: buildIssueLabels(data.category),
        }),
      });

      if (!ghRes.ok) {
        const errText = await ghRes.text();
        return jsonResponse(
          { ok: false, error: `GitHub API error (${ghRes.status}): ${errText.slice(0, 200)}` },
          502,
        );
      }

      const ghJson = await ghRes.json();
      issueUrl = ghJson.html_url || "";
      issueNumber = ghJson.number || null;
    }

    const webhookUrl = String(env.WEBHOOK_URL || "").trim();
    if (webhookUrl) {
      await fetch(webhookUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          content: `New Fluo Feedback: **${title}**${issueUrl ? `\n${issueUrl}` : ""}`,
        }),
      }).catch(() => {});
    }

    if (!githubToken && !webhookUrl) {
      return jsonResponse(
        { ok: false, error: "Relay worker is missing GITHUB_TOKEN or WEBHOOK_URL secret." },
        500,
      );
    }

    return jsonResponse({
      ok: true,
      issue_url: issueUrl,
      issue_number: issueNumber,
      message: issueNumber ? `Created GitHub Issue #${issueNumber}` : "Relayed feedback.",
    });
  },
};
