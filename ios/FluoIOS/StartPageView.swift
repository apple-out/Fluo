import SwiftUI

struct StartPageView: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    var openSettings: () -> Void
    var openBookmarks: () -> Void

    @State private var query = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private struct QuickSite: Identifiable {
        let id: String
        let title: String
        let url: String
        let icon: String
    }

    private var quickSites: [QuickSite] {
        [
            QuickSite(id: "google", title: "Google", url: "https://www.google.com/", icon: "magnifyingglass"),
            QuickSite(id: "wiki", title: "Wikipedia", url: "https://en.wikipedia.org/", icon: "book.closed.fill"),
            QuickSite(id: "namu", title: "나무위키", url: "https://namu.wiki/", icon: "leaf.fill"),
            QuickSite(id: "github", title: "GitHub", url: "https://github.com/", icon: "chevron.left.forwardslash.chevron.right"),
        ]
    }

    private var isModelReady: Bool {
        store.provider != .hyMt2 || catalog.isInstalled(store.selectedLocalModel)
    }

    var body: some View {
        ZStack {
            LiquidAuroraBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Fluo")
                            .font(.system(size: 38, weight: .heavy, design: .rounded))
                            .foregroundStyle(FluoThemeTokens.liquidGradient)

                        Button {
                            openSettings()
                        } label: {
                            HStack(spacing: 6) {
                                Circle()
                                    .fill(isModelReady ? Color.green : Color.orange)
                                    .frame(width: 8, height: 8)
                                Text(
                                    store.provider == .hyMt2
                                        ? t(isModelReady ? "start.modelReady" : "start.modelMissing")
                                        : "\(store.provider.rawValue) · \(store.targetLanguage)"
                                )
                                .font(.caption.weight(.semibold))
                                Image(systemName: "chevron.right")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .contentShape(Capsule())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .liquidGlassPill(tint: isModelReady ? FluoThemeTokens.indigo : .orange, interactive: true)
                    }
                    .padding(.top, 12)

                    HStack(spacing: 10) {
                        Image(systemName: "magnifyingglass")
                            .foregroundStyle(FluoThemeTokens.indigo)
                        TextField(t("bar.urlPlaceholder"), text: $query)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.webSearch)
                            .submitLabel(.go)
                            .onSubmit {
                                let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
                                guard !trimmed.isEmpty else { return }
                                store.activeTab.navigate(input: trimmed, searchEngine: store.userData.searchEngine)
                                query = ""
                            }
                        if !query.isEmpty {
                            Button {
                                query = ""
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 13)
                    .liquidGlass(cornerRadius: 20)

                    VStack(alignment: .leading, spacing: 10) {
                        Text(t("start.quick"))
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.secondary)

                        LiquidGlassGroup(spacing: 10) {
                            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 4), spacing: 10) {
                                ForEach(quickSites) { site in
                                    Button {
                                        store.activeTab.navigate(input: site.url, searchEngine: store.userData.searchEngine)
                                    } label: {
                                        VStack(spacing: 8) {
                                            Image(systemName: site.icon)
                                                .font(.title3.weight(.semibold))
                                                .foregroundStyle(FluoThemeTokens.indigo)
                                            Text(site.title)
                                                .font(.caption2.weight(.semibold))
                                                .lineLimit(1)
                                        }
                                        .frame(maxWidth: .infinity)
                                        .padding(.vertical, 14)
                                        .contentShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())
                                    .liquidGlass(cornerRadius: 18, interactive: true)
                                }
                            }
                        }
                    }

                    if !store.userData.bookmarks.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Text(t("start.favorites"))
                                    .font(.caption.weight(.bold))
                                    .foregroundStyle(.secondary)
                                Spacer()
                                Button(t("menu.bookmarks")) {
                                    openBookmarks()
                                }
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(FluoThemeTokens.indigo)
                            }

                            LiquidGlassGroup(spacing: 10) {
                                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                                    ForEach(Array(store.userData.bookmarks.prefix(8))) { item in
                                        Button {
                                            store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                                        } label: {
                                            VStack(alignment: .leading, spacing: 4) {
                                                Text(item.title)
                                                    .font(.subheadline.weight(.semibold))
                                                    .lineLimit(1)
                                                Text(URL(string: item.url)?.host ?? item.url)
                                                    .font(.caption2)
                                                    .foregroundStyle(.secondary)
                                                    .lineLimit(1)
                                            }
                                            .frame(maxWidth: .infinity, alignment: .leading)
                                            .padding(12)
                                            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                                        }
                                        .buttonStyle(LiquidPressButtonStyle())
                                        .liquidGlass(cornerRadius: 16, interactive: true)
                                    }
                                }
                            }
                        }
                    }

                    if !store.userData.history.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Text(t("start.recent"))
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)

                            VStack(spacing: 0) {
                                ForEach(Array(store.userData.history.prefix(6).enumerated()), id: \.element.id) { idx, entry in
                                    Button {
                                        store.activeTab.navigate(input: entry.url, searchEngine: store.userData.searchEngine)
                                    } label: {
                                        HStack(spacing: 10) {
                                            Image(systemName: "clock")
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                            VStack(alignment: .leading, spacing: 2) {
                                                Text(entry.title)
                                                    .font(.subheadline.weight(.medium))
                                                    .lineLimit(1)
                                                Text(URL(string: entry.url)?.host ?? entry.url)
                                                    .font(.caption2)
                                                    .foregroundStyle(.secondary)
                                                    .lineLimit(1)
                                            }
                                            Spacer()
                                            Image(systemName: "chevron.right")
                                                .font(.caption2)
                                                .foregroundStyle(.secondary)
                                        }
                                        .padding(.horizontal, 14)
                                        .padding(.vertical, 11)
                                        .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                    if idx < min(store.userData.history.count, 6) - 1 {
                                        Divider().opacity(0.22)
                                    }
                                }
                            }
                            .liquidGlass(cornerRadius: 20)
                        }
                    }
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 132)
            }
        }
    }
}
