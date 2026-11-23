import SwiftUI
import UIKit

private struct ReadingShelfEntry: Identifiable {
    let id: String
    let title: String
    let url: String
    let isReadingList: Bool
}

struct StartPageView: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    var openSettings: () -> Void
    var openBookmarks: () -> Void

    @State private var query = ""
    @State private var showAddShortcut = false
    @State private var shortcutTitleDraft = ""
    @State private var shortcutUrlDraft = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var isModelReady: Bool {
        store.provider != .hyMt2 || catalog.isInstalled(store.selectedLocalModel)
    }

    private func cleanHost(for urlString: String) -> String {
        guard let host = URL(string: urlString)?.host, !host.isEmpty else { return urlString }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }

    private var readingShelfEntries: [ReadingShelfEntry] {
        var seen = Set<String>()
        var result: [ReadingShelfEntry] = []

        for bm in store.userData.bookmarks where bm.folder == "Reading List" {
            if seen.insert(bm.url).inserted {
                result.append(ReadingShelfEntry(id: "rl-\(bm.id)", title: bm.title, url: bm.url, isReadingList: true))
            }
        }
        for hist in store.userData.history {
            if seen.insert(hist.url).inserted {
                result.append(ReadingShelfEntry(id: "hi-\(hist.id)", title: hist.title, url: hist.url, isReadingList: false))
            }
            if result.count >= 10 { break }
        }
        return Array(result.prefix(10))
    }

    private var favoriteCards: [BookmarkItem] {
        let primary = store.userData.bookmarks.filter { $0.folder != "Reading List" }
        let pool = primary.isEmpty ? store.userData.bookmarks : primary
        return Array(pool.prefix(8))
    }

    var body: some View {
        ZStack {
            LiquidAuroraBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // Editorial Masthead
                    VStack(alignment: .leading, spacing: 12) {
                        HStack(spacing: 8) {
                            Button {
                                openSettings()
                            } label: {
                                HStack(spacing: 6) {
                                    Circle()
                                        .fill(isModelReady ? FluoThemeTokens.emerald : FluoThemeTokens.amber)
                                        .frame(width: 7, height: 7)
                                    Text(
                                        store.provider == .hyMt2
                                            ? t(isModelReady ? "start.modelReady" : "start.modelMissing")
                                            : "\(store.provider.rawValue) · \(store.targetLanguage)"
                                    )
                                    .font(.caption2.weight(.bold))
                                    Image(systemName: "chevron.right")
                                        .font(.system(size: 9, weight: .bold))
                                        .foregroundStyle(.secondary)
                                }
                                .padding(.horizontal, 12)
                                .padding(.vertical, 6)
                                .contentShape(Capsule())
                            }
                            .buttonStyle(LiquidPressButtonStyle())
                            .liquidGlassPill(
                                tint: isModelReady ? FluoThemeTokens.indigo : FluoThemeTokens.amber,
                                interactive: true
                            )

                            Spacer()

                            if !store.userData.bookmarks.isEmpty {
                                Button {
                                    openBookmarks()
                                } label: {
                                    HStack(spacing: 5) {
                                        Image(systemName: "books.vertical.fill")
                                            .font(.caption2.weight(.bold))
                                            .foregroundStyle(FluoThemeTokens.indigo)
                                        Text("\(store.userData.bookmarks.count)")
                                            .font(.caption2.monospacedDigit().weight(.black))
                                    }
                                    .padding(.horizontal, 11)
                                    .padding(.vertical, 6)
                                    .contentShape(Capsule())
                                }
                                .buttonStyle(LiquidPressButtonStyle())
                                .liquidGlassPill(interactive: true)
                            }
                        }

                        VStack(alignment: .leading, spacing: 4) {
                            Text("Fluo")
                                .font(.system(size: 46, weight: .black, design: .serif))
                                .foregroundStyle(FluoThemeTokens.liquidGradient)
                            Text(t("app.tagline"))
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.top, 10)

                    // Hero Search Portal
                    HStack(spacing: 12) {
                        Image(systemName: "magnifyingglass")
                            .font(.subheadline.weight(.bold))
                            .foregroundStyle(FluoThemeTokens.indigo)

                        TextField(t("bar.urlPlaceholder"), text: $query)
                            .font(.body)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.webSearch)
                            .submitLabel(.go)
                            .onSubmit {
                                submitQuery()
                            }

                        if !query.isEmpty {
                            Button {
                                query = ""
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)

                            Button {
                                submitQuery()
                            } label: {
                                Image(systemName: "arrow.up.right")
                                    .font(.caption.weight(.black))
                                    .foregroundStyle(.white)
                                    .frame(width: 28, height: 28)
                                    .background(FluoThemeTokens.liquidGradient, in: Circle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 18)
                    .padding(.vertical, 15)
                    .liquidGlass(cornerRadius: 26, tint: FluoThemeTokens.indigo)

                    // Horizontal Quick Shortcuts Strip
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Text(t("start.quick"))
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)
                                .textCase(.uppercase)
                            Spacer()
                            Button {
                                shortcutTitleDraft = ""
                                shortcutUrlDraft = ""
                                showAddShortcut = true
                            } label: {
                                Image(systemName: "plus")
                                    .font(.caption.bold())
                                    .foregroundStyle(FluoThemeTokens.indigo)
                                    .frame(width: 28, height: 28)
                                    .contentShape(Circle())
                            }
                            .buttonStyle(LiquidPressButtonStyle())
                            .liquidGlassCircle(tint: FluoThemeTokens.indigo, interactive: true)
                        }

                        ScrollView(.horizontal, showsIndicators: false) {
                            LiquidGlassGroup(spacing: 10) {
                                HStack(spacing: 10) {
                                    ForEach(store.userData.customShortcuts) { site in
                                        let host = cleanHost(for: site.urlString)
                                        Button {
                                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                            store.activeTab.navigate(input: site.urlString, searchEngine: store.userData.searchEngine)
                                        } label: {
                                            VStack(spacing: 9) {
                                                DomainMonogramBadge(
                                                    title: site.title,
                                                    host: host,
                                                    size: 38,
                                                    cornerRadius: 13
                                                )
                                                Text(site.title)
                                                    .font(.caption2.weight(.bold))
                                                    .lineLimit(1)
                                            }
                                            .frame(width: 78, height: 88)
                                            .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                                        }
                                        .buttonStyle(LiquidPressButtonStyle())
                                        .liquidGlass(
                                            cornerRadius: 22,
                                            tint: FluoThemeTokens.domainAccent(for: host),
                                            interactive: true
                                        )
                                        .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: 22, style: .continuous))
                                        .contextMenu {
                                            Button(role: .destructive) {
                                                store.removeCustomShortcut(site.id)
                                            } label: {
                                                Label(t("set.delete"), systemImage: "trash")
                                            }
                                        }
                                    }
                                }
                                .padding(.vertical, 4)
                                .padding(.horizontal, 2)
                            }
                        }
                    }
                    .alert(t("common.add"), isPresented: $showAddShortcut) {
                        TextField(t("bm.nameLabel"), text: $shortcutTitleDraft)
                        TextField(t("bm.urlLabel"), text: $shortcutUrlDraft)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        Button(t("common.cancel"), role: .cancel) {}
                        Button(t("common.add")) {
                            store.addCustomShortcut(title: shortcutTitleDraft, urlString: shortcutUrlDraft)
                        }
                    }

                    // Horizontal Reading Shelf (Reading List + Recent Pages)
                    if !readingShelfEntries.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Text(t("start.recent"))
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)
                                .textCase(.uppercase)

                            ScrollView(.horizontal, showsIndicators: false) {
                                LiquidGlassGroup(spacing: 12) {
                                    HStack(spacing: 12) {
                                        ForEach(readingShelfEntries) { entry in
                                            let host = cleanHost(for: entry.url)
                                            Button {
                                                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                                store.activeTab.navigate(input: entry.url, searchEngine: store.userData.searchEngine)
                                            } label: {
                                                VStack(alignment: .leading, spacing: 10) {
                                                    HStack {
                                                        DomainMonogramBadge(
                                                            title: entry.title,
                                                            host: host,
                                                            size: 32,
                                                            cornerRadius: 10
                                                        )
                                                        Spacer()
                                                        if entry.isReadingList {
                                                            Image(systemName: "eyeglasses")
                                                                .font(.caption.bold())
                                                                .foregroundStyle(FluoThemeTokens.indigo)
                                                                .padding(.horizontal, 8)
                                                                .padding(.vertical, 4)
                                                                .background(FluoThemeTokens.indigo.opacity(0.16), in: Capsule())
                                                        } else {
                                                            Image(systemName: "arrow.up.right")
                                                                .font(.caption2.bold())
                                                                .foregroundStyle(.secondary)
                                                        }
                                                    }

                                                    Spacer(minLength: 0)

                                                    VStack(alignment: .leading, spacing: 3) {
                                                        Text(entry.title.isEmpty ? host : entry.title)
                                                            .font(.subheadline.weight(.bold))
                                                            .lineLimit(2)
                                                            .multilineTextAlignment(.leading)
                                                        Text(host)
                                                            .font(.caption2.weight(.medium))
                                                            .foregroundStyle(.secondary)
                                                            .lineLimit(1)
                                                    }
                                                }
                                                .padding(14)
                                                .frame(width: 214, height: 122, alignment: .topLeading)
                                                .contentShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
                                            }
                                            .buttonStyle(LiquidPressButtonStyle())
                                            .liquidGlass(
                                                cornerRadius: 24,
                                                tint: entry.isReadingList ? FluoThemeTokens.indigo : FluoThemeTokens.domainAccent(for: host),
                                                interactive: true
                                            )
                                        }
                                    }
                                    .padding(.vertical, 4)
                                    .padding(.horizontal, 2)
                                }
                            }
                        }
                    }

                    // Favorites Bento Grid
                    if !favoriteCards.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            HStack {
                                Text(t("start.favorites"))
                                    .font(.caption.weight(.bold))
                                    .foregroundStyle(.secondary)
                                    .textCase(.uppercase)
                                Spacer()
                                Button(t("menu.bookmarks")) {
                                    openBookmarks()
                                }
                                .font(.caption.weight(.bold))
                                .foregroundStyle(FluoThemeTokens.indigo)
                            }

                            LiquidGlassGroup(spacing: 10) {
                                LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                                    ForEach(favoriteCards) { item in
                                        let host = cleanHost(for: item.url)
                                        Button {
                                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                            store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                                        } label: {
                                            HStack(spacing: 10) {
                                                DomainMonogramBadge(
                                                    title: item.title,
                                                    host: host,
                                                    size: 34,
                                                    cornerRadius: 11
                                                )
                                                VStack(alignment: .leading, spacing: 2) {
                                                    Text(item.title)
                                                        .font(.subheadline.weight(.bold))
                                                        .lineLimit(1)
                                                    Text(host)
                                                        .font(.caption2)
                                                        .foregroundStyle(.secondary)
                                                        .lineLimit(1)
                                                }
                                                Spacer(minLength: 0)
                                            }
                                            .padding(12)
                                            .contentShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                                        }
                                        .buttonStyle(LiquidPressButtonStyle())
                                        .liquidGlass(cornerRadius: 20, interactive: true)
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 108)
            }
        }
    }

    private func submitQuery() {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        store.activeTab.navigate(input: trimmed, searchEngine: store.userData.searchEngine)
        query = ""
    }
}
