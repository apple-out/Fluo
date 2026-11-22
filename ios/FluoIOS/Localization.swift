import Foundation

enum AppLanguage: String, CaseIterable, Identifiable, Codable {
    case ko = "ko"
    case en = "en"
    case ja = "ja"
    case zhCN = "zh-CN"

    var id: String { rawValue }

    var nativeName: String {
        switch self {
        case .ko: return "한국어"
        case .en: return "English"
        case .ja: return "日本語"
        case .zhCN: return "简体中文"
        }
    }

    var shortCode: String {
        switch self {
        case .ko: return "KO"
        case .en: return "EN"
        case .ja: return "JA"
        case .zhCN: return "ZH"
        }
    }

    var defaultTargetLanguage: String {
        switch self {
        case .ko: return "한국어"
        case .en: return "English"
        case .ja: return "日本語"
        case .zhCN: return "中文(简体)"
        }
    }

    static func detectSystem() -> AppLanguage {
        let code = Locale.preferredLanguages.first?.lowercased() ?? "en"
        if code.hasPrefix("ko") { return .ko }
        if code.hasPrefix("ja") { return .ja }
        if code.hasPrefix("zh") { return .zhCN }
        return .en
    }
}

enum L10n {
    static func tr(_ lang: AppLanguage, _ key: String) -> String {
        strings[lang]?[key] ?? strings[.en]?[key] ?? key
    }

    private static let strings: [AppLanguage: [String: String]] = [
        .ko: [
            "app.tagline": "Browse in flow. Translate on-device.",
            "onb.title": "표시 언어 및 기본 번역 언어 선택",
            "onb.desc": "Fluo는 에스페란토로 '흐름 · 유창함'을 뜻합니다. 편한 언어를 선택하세요(설정에서 언제든 변경 가능).",
            "onb.targetLabel": "기본 번역 언어",
            "onb.startBtn": "Fluo 시작하기",
            "bar.urlPlaceholder": "검색어 또는 URL 입력",
            "bar.translate": "번역",
            "sheet.translateTitle": "페이지 번역",
            "tr.inline": "인라인 번역",
            "tr.bilingual": "원문과 함께 보기",
            "tr.original": "원문 보기",
            "tr.progress": "번역 중",
            "tr.needModel": "온디바이스 모델이 아직 설치되지 않았습니다. 아래에서 바로 내려받거나 설정에서 클라우드 API를 연결하세요.",
            "tr.downloadRec": "권장 2-Bit 초경량 모델 다운로드 (~572 MB)",
            "tabs.title": "열린 탭",
            "tabs.newTab": "새 탭",
            "set.title": "설정",
            "set.langSection": "표시 언어 · 번역 언어",
            "set.uiLang": "앱 표시 언어",
            "set.targetLang": "번역 대상 언어",
            "set.aiSection": "AI 번역 엔진",
            "set.provider": "번역 방식",
            "set.provider.local": "Hy-MT2 온디바이스 (오프라인 · Apache 2.0)",
            "set.provider.gemini": "Google AI Studio (Gemini)",
            "set.provider.openai": "OpenAI 호환 API",
            "set.modelsSection": "온디바이스 모델 관리 (iOS 메모리 안전)",
            "set.modelsHint": "무료 Apple ID/아이폰 6GB RAM 환경에서는 2-Bit(572MB) 또는 1.25-Bit 모델을 권장합니다.",
            "set.download": "다운로드",
            "set.cancel": "취소",
            "set.delete": "삭제",
            "set.installed": "설치됨 · 선택",
            "set.unload": "메모리 즉시 해제",
            "set.apiKey": "API 키 (iOS Keychain 암호화)",
            "set.baseUrl": "API 기본 URL",
            "set.modelId": "모델 ID",
            "set.saveKey": "키 저장",
            "set.clearKey": "키 삭제",
            "common.done": "완료",
        ],
        .en: [
            "app.tagline": "Browse in flow. Translate on-device.",
            "onb.title": "Choose Display & Target Language",
            "onb.desc": "Fluo means 'flow & fluency' in Esperanto. Pick your preferred UI language (changeable anytime in Settings).",
            "onb.targetLabel": "Default Target Translation Language",
            "onb.startBtn": "Start Browsing with Fluo",
            "bar.urlPlaceholder": "Search or enter URL",
            "bar.translate": "Translate",
            "sheet.translateTitle": "Page Translation",
            "tr.inline": "Translate Inline",
            "tr.bilingual": "Bilingual View",
            "tr.original": "Restore Original",
            "tr.progress": "Translating",
            "tr.needModel": "No on-device model downloaded yet. Download the recommended 2-Bit model below or pick a Cloud API in Settings.",
            "tr.downloadRec": "Download 2-Bit Light Model (~572 MB)",
            "tabs.title": "Open Tabs",
            "tabs.newTab": "New Tab",
            "set.title": "Settings",
            "set.langSection": "Language",
            "set.uiLang": "App UI Language",
            "set.targetLang": "Target Translation Language",
            "set.aiSection": "AI Translation Engine",
            "set.provider": "Provider",
            "set.provider.local": "Hy-MT2 On-Device (Offline · Apache 2.0)",
            "set.provider.gemini": "Google AI Studio (Gemini)",
            "set.provider.openai": "OpenAI-Compatible API",
            "set.modelsSection": "On-Device Model Manager (iOS Safe)",
            "set.modelsHint": "2-Bit (572 MB) or 1.25-Bit is recommended on iOS (especially with free Apple ID signing limits).",
            "set.download": "Download",
            "set.cancel": "Cancel",
            "set.delete": "Delete",
            "set.installed": "Installed",
            "set.unload": "Unload Model from RAM",
            "set.apiKey": "API Key (iOS Keychain)",
            "set.baseUrl": "Base URL",
            "set.modelId": "Model ID",
            "set.saveKey": "Save Key",
            "set.clearKey": "Clear Key",
            "common.done": "Done",
        ],
        .ja: [
            "app.tagline": "Browse in flow. Translate on-device.",
            "onb.title": "表示言語と翻訳先言語を選択",
            "onb.desc": "Fluo はエスペラントで『流れ・流暢さ』を意味します。使いやすいUI言語をお選びください。",
            "onb.targetLabel": "既定の翻訳先言語",
            "onb.startBtn": "Fluo を始める",
            "bar.urlPlaceholder": "検索または URL を入力",
            "bar.translate": "翻訳",
            "sheet.translateTitle": "ページ翻訳",
            "tr.inline": "インライン翻訳",
            "tr.bilingual": "原文と並列表示",
            "tr.original": "原文に戻す",
            "tr.progress": "翻訳中",
            "tr.needModel": "ローカルモデルが未導入です。下の推奨軽量モデルを取得するか、設定でクラウド API を指定してください。",
            "tr.downloadRec": "推奨 2-Bit 軽量モデルをダウンロード (~572 MB)",
            "tabs.title": "開いているタブ",
            "tabs.newTab": "新しいタブ",
            "set.title": "設定",
            "set.langSection": "言語",
            "set.uiLang": "表示言語",
            "set.targetLang": "翻訳先言語",
            "set.aiSection": "AI 翻訳エンジン",
            "set.provider": "翻訳方式",
            "set.provider.local": "Hy-MT2 オンデバイス (オフライン · Apache 2.0)",
            "set.provider.gemini": "Google AI Studio (Gemini)",
            "set.provider.openai": "OpenAI 互換 API",
            "set.modelsSection": "オンデバイスモデル管理",
            "set.modelsHint": "iPhone のメモリ制限対策として 2-Bit (572 MB) または 1.25-Bit モデルを推奨します。",
            "set.download": "ダウンロード",
            "set.cancel": "キャンセル",
            "set.delete": "削除",
            "set.installed": "導入済み",
            "set.unload": "メモリから解放",
            "set.apiKey": "API キー (iOS Keychain)",
            "set.baseUrl": "ベース URL",
            "set.modelId": "モデル ID",
            "set.saveKey": "保存",
            "set.clearKey": "削除",
            "common.done": "完了",
        ],
        .zhCN: [
            "app.tagline": "Browse in flow. Translate on-device.",
            "onb.title": "选择界面语言与默认译文语言",
            "onb.desc": "Fluo 源自世界语“流畅与流动”，请先选择界面语言（日后可在设置随时更改）。",
            "onb.targetLabel": "默认目标翻译语言",
            "onb.startBtn": "开始使用 Fluo",
            "bar.urlPlaceholder": "搜索或输入网址",
            "bar.translate": "翻译",
            "sheet.translateTitle": "页面翻译",
            "tr.inline": "原位翻译",
            "tr.bilingual": "双语对照",
            "tr.original": "恢复原文",
            "tr.progress": "翻译中",
            "tr.needModel": "尚未下载本地离线模型。可直接下载下方推荐模型，或在设置中连接云端 API。",
            "tr.downloadRec": "下载推荐 2-Bit 超轻模型 (~572 MB)",
            "tabs.title": "打开的标签页",
            "tabs.newTab": "新建标签页",
            "set.title": "设置",
            "set.langSection": "语言",
            "set.uiLang": "界面语言",
            "set.targetLang": "目标翻译语言",
            "set.aiSection": "AI 翻译引擎",
            "set.provider": "提供方",
            "set.provider.local": "Hy-MT2 本地离线 (Apache 2.0)",
            "set.provider.gemini": "Google AI Studio (Gemini)",
            "set.provider.openai": "OpenAI 兼容接口",
            "set.modelsSection": "本地离线模型管理",
            "set.modelsHint": "受 iOS 内存限制影响，强烈推荐使用 2-Bit (572 MB) 或 1.25-Bit 超轻模型。",
            "set.download": "下载",
            "set.cancel": "取消",
            "set.delete": "删除",
            "set.installed": "已安装",
            "set.unload": "立即释放内存",
            "set.apiKey": "API 密钥 (iOS Keychain)",
            "set.baseUrl": "基础 URL",
            "set.modelId": "模型 ID",
            "set.saveKey": "保存密钥",
            "set.clearKey": "清除密钥",
            "common.done": "完成",
        ],
    ]
}
