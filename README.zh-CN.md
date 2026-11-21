[English](./README.md) · [한국어](./README.ko.md) · [日本語](./README.ja.md) · [简体中文](./README.zh-CN.md) · [使用条款](./TERMS_OF_USE.md) · [开源协议与模型政策](./THIRD_PARTY_LICENSES.md)

# Fluo — *Browse in flow. Translate on-device.*
> **畅享无阻浏览，设备端即时翻译。**

> 🌐 **项目主页（支持四语切换）：** https://apple-out.github.io/transratebrowser/  
> 📦 **Windows 公开测试版下载：** https://github.com/apple-out/transratebrowser/releases  
> 🐞 **问题与误译反馈（GitHub Issues）：** https://github.com/apple-out/transratebrowser/issues/new/choose

**Fluo** *(世界语“流畅、流利”之意)* 是一款基于 Windows 原生 WebView2 的轻量级智能网页翻译浏览器，内置基于量子化 Apache 2.0 GGUF 本地大模型（`Hy-MT2` / `llama.cpp` + Vulkan）的**离线原地无损翻译**，同时兼容 **Google Gemini**、**OpenAI 兼容接口**、**OpenAI Legacy / ChatGPT OAuth** 与**本地桥接**。

浏览器支持 **首次启动语言选择**、**简体中文 / 한국어 / English / 日本語 四国语言界面热切换**，并自动适配几乎所有 **CPU + GPU 硬件组合**。

---

## 核心特性

- **首次启动语言选择 & 四语热切换：** 首次打开弹出直观的四语与目标译文语言选择器，日后也可随时在 **菜单 (⋯)** 或 **设置 → 外观与语言** 中一键切换。
- **本地离线 AI (`Hy-MT2`, Apache 2.0) + 全硬件通用加速：**
  - 无需付费 API Key、原文不出本机即可免费离线翻译。
  - **Vulkan GPU 加速：** 全面兼容 **NVIDIA (GeForce/RTX)**、**AMD (Radeon 独显与锐龙核显混合层切分)** 及 **Intel (Arc / Iris Xe)**。
  - **动态全指令集 CPU 后端（`GGML_BACKEND_DL=ON` + `GGML_CPU_ALL_VARIANTS=ON`）：** 内置 `AVX-512`、`AVX2`、`AVX`、`SSE4.2` 与基础 `x64` 运行库，启动时自动选用最快的指令集。
- **保留原始排版的原地替换翻译：** 译文直接嵌入段落并保留超链接、行内代码与交互控件；支持原文/译文对照模式、**框选区域翻译**、右键划词即时译、文体自动识别（`auto` / `natural` / `novel` / `literal` / `polished`）与自定义术语库。
- **完整现代浏览体验：** 非阻塞工作区管理、闲置标签页休眠省内存、网站白名单广告拦截、弹窗控制、浏览器书签/历史/密码 CSV 一键迁移。
- **Windows DPAPI 加密与安全同步：** 所有 API 密钥与登录凭据均通过 Windows DPAPI 本地加密，绝不写入同步档案；支持通过 Google Drive、OneDrive 或自定义目录多端合并同步。
- **无上限公测反馈系统：** 绝不暴露开发者个人邮箱，支持 **GitHub Issues 一键预填提交** 及每日 10 万次免费额度的 **Cloudflare Workers (`*.workers.dev`)** 匿名中继（详见 [FEEDBACK_RELAY_GUIDE.md](./scripts/FEEDBACK_RELAY_GUIDE.md)）。

---

## 快速开始 (Windows 10 / 11 x64)

1. 前往 [GitHub Releases](https://github.com/apple-out/transratebrowser/releases) 下载最新便携包 `Fluo-v0.1.0-win64.zip`。
2. 解压至任意目录（确保 `llama.dll`、`ggml*.dll` 与 VC++ 运行库留在同一文件夹内），双击 **`Fluo.exe`** 即可运行。
3. 在首次启动语言选择弹窗选定语言后，点击工具栏 **`翻译 ▾` → 下载推荐模型** 即可启用离线本地翻译。

使用条款（含可选 OpenAI Legacy / ChatGPT OAuth 连接说明）请见 [TERMS_OF_USE.md](./TERMS_OF_USE.md)，第三方依赖及 Apache 2.0 开源模型授权政策请见 [THIRD_PARTY_LICENSES.md](./THIRD_PARTY_LICENSES.md)。

## 开源许可

[MIT License](./LICENSE)
