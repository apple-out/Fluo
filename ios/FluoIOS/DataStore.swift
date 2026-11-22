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
}

struct FluoUserDataSnapshot: Codable {
    var workspaces: [WorkspaceItem] = [WorkspaceItem(id: "default", name: "General")]
    var activeWorkspaceId: String = "default"
    var bookmarks: [BookmarkItem] = []
    var history: [HistoryEntry] = []
    var translationHistory: [TranslationHistoryEntry] = []
    var glossary: [GlossaryItem] = []
    var searchEngine: SearchEngine = .google
    var appTheme: AppTheme = .system
    var style: TranslationStyle = .auto
    var fontScale: Double = 1.0
    var lineHeight: Double = 1.6
    var adBlockEnabled: Bool = true
    var adBlockAllowlist: [String] = []

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
