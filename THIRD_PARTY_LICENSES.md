# Third-Party Open Source Licenses & AI Model Policy Notice

**Fluo** (*"Browse in flow. Translate on-device."*) is licensed under the **MIT License** ([`LICENSE`](./LICENSE)).  
Below is a complete audit of third-party libraries, native runtimes, AI models, and external service policies used or referenced by Fluo.

---

## 1. Core Native Inference & Graphics Stack (Bundled Runtime DLLs)

| Component | Purpose | License | Source / Repository |
| :--- | :--- | :--- | :--- |
| **`llama.cpp` & `ggml`** | On-device GGUF LLM inference engine (`llama.dll`, `ggml*.dll`, universal CPU & Vulkan GPU backends) | **MIT License** | https://github.com/ggml-org/llama.cpp |
| **`llama-cpp-2` / `llama-cpp-sys-2`** | Rust FFI bindings (`v0.1.161`) | **MIT OR Apache-2.0** | https://crates.io/crates/llama-cpp-2 |
| **Vulkan Headers & Loader (`vulkan-1.lib`)** | Cross-vendor GPU compute abstraction (NVIDIA / AMD / Intel) | **Apache-2.0 / MIT** | https://github.com/KhronosGroup/Vulkan-Headers |
| **Microsoft Visual C++ Runtime DLLs** *(Portable ZIP only: `msvcp140.dll`, `vcruntime140.dll`, `vcruntime140_1.dll`)* | MSVC C++ runtime for App-Local portable deployment on clean Windows systems | **Microsoft Visual Studio Redistributable Terms** (Proprietary, redistributable with compiled binaries built via licensed Visual Studio) | https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files |

> **Distribution Note on MSVC Runtime DLLs:** Per Microsoft's App-Local Deployment terms, these runtime DLLs are excluded from the Git source tree via `.gitignore` and included only inside compiled portable release archives alongside `Fluo.exe` so testers do not require administrator installers.

```text
llama.cpp & ggml Copyright Notice (MIT License)
Copyright (c) 2023-2026 The ggml authors
Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software
without restriction, including without limitation the rights to use, copy, modify,
merge, publish, distribute, sublicense, and/or sell copies of the Software...
```

---

## 2. Application Framework, Embedded Database & WebView

| Component | Purpose | License | Source / Repository |
| :--- | :--- | :--- | :--- |
| **Tauri (`v2`) & `tauri-plugin-opener`** | Desktop shell, IPC, OS integration (`tauri`, `tauri-build`, `@tauri-apps/api`, `@tauri-apps/cli`) | **MIT OR Apache-2.0** | https://github.com/tauri-apps/tauri |
| **Svelte 5 / SvelteKit / Vite** | Browser UI & embedded settings framework | **MIT License** | https://github.com/sveltejs/svelte |
| **`rusqlite` (bundled SQLite)** | Local bookmarks, history, glossary, translation cache DB | **MIT** (`rusqlite`) + **Public Domain** (`SQLite3`) | https://github.com/rusqlite/rusqlite |
| **`reqwest` / `tokio` / `rustls` / `aws-lc-rs`** | Async HTTP & TLS stack (`reqwest 0.13`, `tokio`, `rustls`, `aws-lc-rs`) | **MIT / Apache-2.0 / ISC** | https://crates.io |
| **`webview2-com` / `windows` (`windows-rs`)** | Windows WebView2 COM, DXGI, DPAPI (`CryptProtectData`), OS locale APIs | **MIT OR Apache-2.0** | https://github.com/microsoft/windows-rs |
| **Microsoft Edge WebView2 Runtime** | Evergreen OS-provided rendering engine (linked dynamically, **not bundled**) | **Microsoft Software License Terms** | https://developer.microsoft.com/microsoft-edge/webview2/ |

---

## 3. Transitive Dependency License Audit Summary

An automated audit of the full Rust crate graph (`cargo metadata --features vulkan,universal-cpu`) and Node build dependency tree (`tauri-app/node_modules`) confirmed:

- **Zero GPL or AGPL dependencies** in the application binary or frontend bundle.
- **Dual-licensed option crates:** `r-efi` is licensed under `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; Fluo elects **MIT / Apache-2.0**.
- **Weak File-Level Copyleft (`MPL-2.0`):**
  - Transitive Rust crates (via `urlpattern` / Tauri): `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`
  - Build-time frontend dependency (not shipped in runtime binary): `lightningcss`, `lightningcss-win32-x64-msvc`
  - Under **MPL-2.0 §3.3**, combining unmodified MPL-2.0 library files with an MIT project is explicitly permitted. If you distribute modified versions of those upstream MPL-2.0 files, the modified files remain under MPL-2.0.
- **Permissive Majority:** Over 450 transitive Rust crates and 55+ npm packages use **MIT**, **Apache-2.0**, **BSD-3-Clause**, **ISC**, **Zlib**, or **Unicode-3.0**.

---

## 4. Supported AI Models & External Provider Policies

> **Important:** **No AI model weights are bundled** in this Git repository or in release ZIPs. Models are downloaded directly by users on demand from their official upstream repositories, and users are responsible for complying with each model/provider's license.

### A. On-Device GGUF Model Catalog (`hy_mt2.rs`)

| Catalog Model | Upstream Repo | License | Commercial Use |
| :--- | :--- | :--- | :--- |
| **`Hy-MT2-1.8B` (`Q4_K_M`, `Q6_K`, `2Bit`, `1.25Bit`)** *(Default Recommended)* | `tencent/Hy-MT2-1.8B-GGUF` (Hugging Face) | **Apache License 2.0** | **Permitted** |
| **`Hy-MT2-7B` (`Q4_K_M`)** | `tencent/Hy-MT2-7B-GGUF` (Hugging Face) | **Apache License 2.0** | **Permitted** |

### B. Optional Cloud AI Connectors (Bring-Your-Own-Key)

| Provider Option | Auth / Endpoint | Policy & Compliance Notes |
| :--- | :--- | :--- |
| **Google AI Studio (Gemini)** | Official user API key | Subject to Google Generative AI / Gemini API Terms of Service. |
| **OpenAI Official / Compatible API** | User API key / custom base URL | Subject to OpenAI Terms of Use (or your self-hosted OpenAI-compatible server license). |
| **OpenAI Legacy / ChatGPT OAuth (`codexLegacy`)** | User-initiated local OAuth PKCE (`auth.openai.com`) | Optional legacy connector (similar to OpenCode / Codex CLI). Subject to **OpenAI Terms of Use** and regional/account policies; see [`TERMS_OF_USE.md`](./TERMS_OF_USE.md) for full notice. Use an official OpenAI Platform API key (`openAiCompatible`) for commercial or SLA-backed workloads. |
| **Local Bridge** | Loopback HTTP + local bearer token | Connects only to user-run loopback endpoints (`127.0.0.0/8`, `::1`, `localhost`). |
