# Fluo — Terms of Use / 이용 약관

> **Fluo** (*"Browse in flow. Translate on-device."*)  
> Last Updated: 2026-11-21

---

## English

### 1. Software License & "AS IS" Disclaimer
Fluo is open-source software distributed under the **MIT License** (see [`LICENSE`](./LICENSE)). By downloading, building, or running Fluo, you acknowledge that the software is provided **"AS IS"**, without warranty of any kind, express or implied, including but not limited to merchantability, fitness for a particular purpose, translation accuracy, or non-infringement. The authors and contributors are not liable for any direct, indirect, incidental, or consequential damages arising from your use of Fluo.

### 2. On-Device AI Models (`Hy-MT2`)
Fluo does **not** bundle AI model weights in this repository or in release archives. When you choose to download an on-device translation model (e.g., Tencent `Hy-MT2-1.8B` / `Hy-MT2-7B` GGUF variants from Hugging Face), the model is fetched directly to your device under its upstream **Apache License 2.0** terms (see [`THIRD_PARTY_LICENSES.md`](./THIRD_PARTY_LICENSES.md)). You are responsible for local storage, hardware usage, bandwidth, and compliance with the model's license.

### 3. Third-Party Cloud AI Providers (BYOK)
Fluo optionally lets you connect your own API keys or endpoints (such as **Google AI Studio / Gemini**, **OpenAI Platform / OpenAI-compatible APIs**, or a **Local Bridge**).
- You are solely responsible for obtaining valid credentials, paying any third-party usage fees, and complying with each provider's Terms of Service and Acceptable Use Policies.
- Text you choose to translate through a cloud provider is sent directly from your device to that provider's endpoint; Fluo's developers never proxy, inspect, or store your prompts or translations.

### 4. OpenAI Legacy / ChatGPT OAuth Connection (`codexLegacy`)
Fluo includes an optional **OpenAI Legacy / ChatGPT OAuth (`codexLegacy`)** connector for compatibility with user-authorized ChatGPT / Codex OAuth sessions (similar to open-source tools such as OpenCode and Codex CLI).
- **User-Initiated Local Auth:** Connecting via `codexLegacy` is strictly optional and initiated solely by you. OAuth access/refresh tokens are encrypted locally via **Windows DPAPI** (`CryptProtectData`) and are never transmitted to Fluo's developers or included in sync snapshots.
- **Compliance & Risk Responsibility:** This connector uses a legacy / third-party OAuth client flow that is **not officially endorsed, sponsored, or affiliated with OpenAI**. Your use of `codexLegacy` is subject to **OpenAI's Terms of Use**, Usage Policies, rate limits, and regional eligibility rules (which may result in errors such as `403 unsupported_country_region_territory`, session revocation, or account restrictions).
- **Recommended Alternative:** For commercial, production, or SLA-backed workloads — or if you want guaranteed official API compliance — use the **OpenAI Official API (`openAiCompatible`)** with your own OpenAI Platform API key, or use Fluo's free, offline **Hy-MT2 on-device** engine.

### 5. Privacy, Local Storage & Feedback
- Fluo collects **no telemetry, analytics, or browsing history** on developer servers.
- Sensitive credentials (API keys, OAuth tokens, saved logins) are stored exclusively on your PC encrypted with Windows DPAPI and are excluded from folder sync files.
- Submitting bug or translation reports via **GitHub Issues** is voluntary and creates a public issue on GitHub; review your report before posting to ensure it contains no private or sensitive information.

---

## 한국어

### 1. 소프트웨어 라이선스 및 무보증("AS IS") 고지
Fluo는 **MIT 라이선스**([`LICENSE`](./LICENSE))로 배포되는 오픈소스 소프트웨어입니다. Fluo를 다운로드·빌드·실행함으로써, 본 소프트웨어가 상품성, 특정 목적 적합성, 번역 정확성 또는 비침해성에 대한 어떠한 명시적·묵시적 보증 없이 **"있는 그대로(AS IS)"** 제공됨을 이해하고 동의하는 것으로 간주됩니다. 개발자 및 기여자는 소프트웨어 사용으로 인해 발생하는 직·간접적 손해에 대해 책임을 지지 않습니다.

### 2. 온디바이스 AI 모델 (`Hy-MT2`)
Fluo의 Git 저장소와 배포 압축 파일에는 AI 모델 가중치가 포함되어 있지 않습니다. 사용자가 앱 내에서 온디바이스 번역 모델(Hugging Face의 Tencent `Hy-MT2-1.8B` / `Hy-MT2-7B` GGUF 등)을 직접 다운로드하는 경우, 해당 모델은 원본 **Apache License 2.0** 조건에 따라 사용자 기기에 저장됩니다([`THIRD_PARTY_LICENSES.md`](./THIRD_PARTY_LICENSES.md) 참고). 모델 저장 공간, 하드웨어 자원 사용 및 라이선스 준수 책임은 사용자에게 있습니다.

### 3. 제3자 클라우드 AI 제공자 (사용자 키 직접 연결)
Fluo는 사용자가 직접 발급한 API 키 또는 엔드포인트(**Google AI Studio / Gemini**, **OpenAI 공식 · 호환 API**, **로컬 브리지**)를 선택적으로 연결할 수 있는 기능을 제공합니다.
- 각 외부 서비스의 계정 자격 증명 관리, API 과금, 이용약관 및 사용 정책 준수 책임은 전적으로 사용자에게 있습니다.
- 클라우드 API를 통해 번역하는 텍스트는 사용자 PC에서 해당 AI 제공자 서버로 직접 전송되며, Fluo 개발자 서버를 거치거나 수집되지 않습니다.

### 4. OpenAI 레거시 / ChatGPT OAuth 연결 (`codexLegacy`) 안내
Fluo는 OpenCode·Codex CLI 등 오픈소스 생태계와 유사한 방식으로 사용자가 직접 인증한 세션을 활용할 수 있도록 **OpenAI 레거시 / ChatGPT OAuth (`codexLegacy`)** 연결 옵션을 유지하고 있습니다.
- **사용자 주도 로컬 인증:** `codexLegacy` 연결은 완전한 선택 기능이며 사용자의 직접 로그인 또는 로컬 세션 가져오기를 통해서만 활성화됩니다. 발급된 OAuth 토큰은 **Windows DPAPI**(`CryptProtectData`)로 로컬에만 암호화 저장되며 개발자나 동기화 파일로 전송되지 않습니다.
- **약관 준수 및 책임 고지:** 본 기능은 OpenAI가 공식 보증하거나 제휴한 클라이언트가 아닌 레거시 OAuth 연동 방식입니다. 해당 연결의 사용은 **OpenAI 이용약관(Terms of Use)**, 사용 정책, 호출 제한 및 지역 제한 정책의 적용을 받으며, OpenAI 정책에 따라 지역 차단(`403 unsupported_country_region_territory`), 세션 만료 또는 계정 제재가 발생할 수 있습니다. 이에 따른 모든 책임은 사용자에게 있습니다.
- **권장 대안:** 상업적 목적, 프로덕션 환경 또는 공식 API 약관 보장이 필요한 경우에는 **OpenAI 공식 API (`openAiCompatible`)**에 OpenAI Platform API 키를 입력해 사용하거나, 완전 무료·오프라인인 **Hy-MT2 온디바이스 엔진**을 사용하시기 바랍니다.

### 5. 개인정보 보호, 로컬 저장 및 오픈 베타 피드백
- Fluo는 개발자 서버로 원격 측정(텔레메트리), 사용 통계 또는 방문 기록을 수집하지 않습니다.
- API 키·OAuth 토큰·저장된 로그인 정보는 Windows DPAPI로 암호화되어 사용자 PC에만 보관되며, 클라우드 폴더 동기화 스냅샷에는 절대 포함되지 않습니다.
- **GitHub Issues**를 통한 버그·오역 제보는 전적으로 사용자의 선택이며 공개 이슈로 등록되므로, 게시 전 개인정보나 민감한 내용이 포함되지 않았는지 확인해 주세요.
