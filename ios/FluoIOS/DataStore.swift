import Foundation

enum SearchEngine: String, CaseIterable, Identifiable, Codable {
    case google
    case bing
    case duckduckgo

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .google: return "Google"
        case .bing: return "Bing"
        case .duckduckgo: return "DuckDuckGo"
        }
    }

    var homeURL: String {
        switch self {
        case .google: return "https://www.google.com/"
        case .bing: return "https://www.bing.com/"
        case .duckduckgo: return "https://duckduckgo.com/"
        }
    }

    func searchURL(query: String) -> URL? {
        let q = query.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? query
        switch self {
        case .google: return URL(string: "https://www.google.com/search?q=\(q)")
        case .bing: return URL(string: "https://www.bing.com/search?q=\(q)")
        case .duckduckgo: return URL(string: "https://duckduckgo.com/?q=\(q)")
        }
    }
}

enum AppTheme: String, CaseIterable, Identifiable, Codable {
    case system
    case light
    case dark

    var id: String { rawValue }
}

enum TranslationStyle: String, CaseIterable, Identifiable, Codable {
    case auto
    case natural
    case novel
    case literal
    case polished

    var id: String { rawValue }
}

struct WorkspaceItem: Identifiable, Codable, Hashable {
    var id: String
    var name: String
}

struct BookmarkItem: Identifiable, Codable, Hashable {
    var id: String
    var url: String
    var title: String
    var folder: String
    var createdAt: Date
}

struct HistoryEntry: Identifiable, Codable, Hashable {
    var id: String
    var url: String
    var title: String
    var visitedAt: Date
}

struct TranslationHistoryEntry: Identifiable, Codable, Hashable {
    var id: String
    var url: String
    var title: String
    var targetLanguage: String
    var provider: String
    var translatedAt: Date
}

struct GlossaryItem: Identifiable, Codable, Hashable {
    var id: String
    var source: String
    var target: String
    var caseSensitive: Bool
    var folder: String = "Default"
    var enabled: Bool = true
    var exactMatch: Bool = false
    var useRegex: Bool = false
    var doNotTranslate: Bool = false
    var note: String = ""

    init(
        id: String,
        source: String,
        target: String,
        caseSensitive: Bool,
        folder: String = "Default",
        enabled: Bool = true,
        exactMatch: Bool = false,
        useRegex: Bool = false,
        doNotTranslate: Bool = false,
        note: String = ""
    ) {
        self.id = id
        self.source = source
        self.target = target
        self.caseSensitive = caseSensitive
        self.folder = folder.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Default" : folder
        self.enabled = enabled
        self.exactMatch = exactMatch
        self.useRegex = useRegex
        self.doNotTranslate = doNotTranslate
        self.note = note
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        source = try c.decode(String.self, forKey: .source)
        target = try c.decode(String.self, forKey: .target)
        caseSensitive = try c.decodeIfPresent(Bool.self, forKey: .caseSensitive) ?? false
        let rawFolder = try c.decodeIfPresent(String.self, forKey: .folder) ?? "Default"
        folder = rawFolder.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Default" : rawFolder
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        exactMatch = try c.decodeIfPresent(Bool.self, forKey: .exactMatch) ?? false
        useRegex = try c.decodeIfPresent(Bool.self, forKey: .useRegex) ?? false
        doNotTranslate = try c.decodeIfPresent(Bool.self, forKey: .doNotTranslate) ?? false
        note = try c.decodeIfPresent(String.self, forKey: .note) ?? ""
    }
}

struct SavedTabItem: Identifiable, Codable, Hashable {
    var id: String
    var urlString: String
    var title: String
    var workspaceId: String
    var isPinned: Bool
}

struct QuickShortcutItem: Identifiable, Codable, Hashable {
    var id: String
    var title: String
    var urlString: String
    var icon: String
    var tintHex: String

    static let defaults: [QuickShortcutItem] = [
        QuickShortcutItem(id: "namu", title: "나무위키", urlString: "https://namu.wiki", icon: "book.pages.fill", tintHex: "#00A495"),
        QuickShortcutItem(id: "wikipedia", title: "Wikipedia", urlString: "https://en.wikipedia.org", icon: "globe.americas.fill", tintHex: "#3B82F6"),
        QuickShortcutItem(id: "nhk", title: "NHK News", urlString: "https://www3.nhk.or.jp/news/", icon: "newspaper.fill", tintHex: "#EC4899"),
        QuickShortcutItem(id: "hackernews", title: "Hacker News", urlString: "https://news.ycombinator.com", icon: "bolt.horizontal.circle.fill", tintHex: "#F97316"),
        QuickShortcutItem(id: "syosetu", title: "小説家になろう", urlString: "https://syosetu.com", icon: "text.book.closed.fill", tintHex: "#8B5CF6"),
        QuickShortcutItem(id: "github", title: "GitHub", urlString: "https://github.com/trending", icon: "chevron.left.forwardslash.chevron.right", tintHex: "#10B981"),
    ]
}

struct DownloadItemRecord: Identifiable, Codable, Hashable {
    var id: String
    var fileName: String
    var sourceURL: String
    var localPath: String
    var byteSize: Int64
    var createdAt: Date
}

struct FluoUserDataSnapshot: Codable {
    static let startPageURL = "fluo://start"

    var workspaces: [WorkspaceItem] = [WorkspaceItem(id: "default", name: "General")]
    var activeWorkspaceId: String = "default"
    var bookmarks: [BookmarkItem] = []
    var history: [HistoryEntry] = []
    var translationHistory: [TranslationHistoryEntry] = []
    var glossary: [GlossaryItem] = []
    var glossaryFolders: [String] = ["Default"]
    var searchEngine: SearchEngine = .google
    var appTheme: AppTheme = .system
    var style: TranslationStyle = .auto
    var fontScale: Double = 1.0
    var lineHeight: Double = 1.6
    var adBlockEnabled: Bool = true
    var adBlockAllowlist: [String] = []
    var savedTabs: [SavedTabItem] = []
    var activeSavedTabId: String? = nil
    var customShortcuts: [QuickShortcutItem] = QuickShortcutItem.defaults
    var downloads: [DownloadItemRecord] = []
    var autoTranslateEnabled: Bool = true
    var autoTranslateHosts: [String] = []
    var neverTranslateHosts: [String] = []

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let def = FluoUserDataSnapshot()
        workspaces = try c.decodeIfPresent([WorkspaceItem].self, forKey: .workspaces) ?? def.workspaces
        activeWorkspaceId = try c.decodeIfPresent(String.self, forKey: .activeWorkspaceId) ?? def.activeWorkspaceId
        bookmarks = try c.decodeIfPresent([BookmarkItem].self, forKey: .bookmarks) ?? def.bookmarks
        history = try c.decodeIfPresent([HistoryEntry].self, forKey: .history) ?? def.history
        translationHistory = try c.decodeIfPresent([TranslationHistoryEntry].self, forKey: .translationHistory) ?? def.translationHistory
        glossary = try c.decodeIfPresent([GlossaryItem].self, forKey: .glossary) ?? def.glossary
        var folders = try c.decodeIfPresent([String].self, forKey: .glossaryFolders) ?? ["Default"]
        if !folders.contains("Default") {
            folders.insert("Default", at: 0)
        }
        for item in glossary where !folders.contains(item.folder) {
            folders.append(item.folder)
        }
        glossaryFolders = folders
        searchEngine = try c.decodeIfPresent(SearchEngine.self, forKey: .searchEngine) ?? def.searchEngine
        appTheme = try c.decodeIfPresent(AppTheme.self, forKey: .appTheme) ?? def.appTheme
        style = try c.decodeIfPresent(TranslationStyle.self, forKey: .style) ?? def.style
        fontScale = try c.decodeIfPresent(Double.self, forKey: .fontScale) ?? def.fontScale
        lineHeight = try c.decodeIfPresent(Double.self, forKey: .lineHeight) ?? def.lineHeight
        adBlockEnabled = try c.decodeIfPresent(Bool.self, forKey: .adBlockEnabled) ?? def.adBlockEnabled
        adBlockAllowlist = try c.decodeIfPresent([String].self, forKey: .adBlockAllowlist) ?? def.adBlockAllowlist
        savedTabs = try c.decodeIfPresent([SavedTabItem].self, forKey: .savedTabs) ?? []
        activeSavedTabId = try c.decodeIfPresent(String.self, forKey: .activeSavedTabId)
        customShortcuts = try c.decodeIfPresent([QuickShortcutItem].self, forKey: .customShortcuts) ?? QuickShortcutItem.defaults
        downloads = try c.decodeIfPresent([DownloadItemRecord].self, forKey: .downloads) ?? []
        autoTranslateEnabled = try c.decodeIfPresent(Bool.self, forKey: .autoTranslateEnabled) ?? true
        autoTranslateHosts = try c.decodeIfPresent([String].self, forKey: .autoTranslateHosts) ?? []
        neverTranslateHosts = try c.decodeIfPresent([String].self, forKey: .neverTranslateHosts) ?? []
    }

    func exportJSONString() -> String? {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(self) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func importJSONString(_ raw: String) -> FluoUserDataSnapshot? {
        guard let data = raw.data(using: .utf8),
              var decoded = try? JSONDecoder().decode(FluoUserDataSnapshot.self, from: data) else {
            return nil
        }
        if decoded.workspaces.isEmpty {
            decoded.workspaces = [WorkspaceItem(id: "default", name: "General")]
            decoded.activeWorkspaceId = "default"
        }
        return decoded
    }

    private static var fileURL: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        if !FileManager.default.fileExists(atPath: base.path) {
            try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        }
        return base.appendingPathComponent("FluoUserData.json")
    }

    static func load() -> FluoUserDataSnapshot {
        guard let data = try? Data(contentsOf: fileURL),
              var decoded = try? JSONDecoder().decode(FluoUserDataSnapshot.self, from: data) else {
            return FluoUserDataSnapshot()
        }
        if decoded.workspaces.isEmpty {
            decoded.workspaces = [WorkspaceItem(id: "default", name: "General")]
            decoded.activeWorkspaceId = "default"
        }
        return decoded
    }

    func save() {
        if let data = try? JSONEncoder().encode(self) {
            try? data.write(to: Self.fileURL, options: [.atomic])
        }
    }
}
