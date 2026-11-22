import SwiftUI
import UIKit

struct OnboardingSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Fluo")
                                .font(.system(size: 36, weight: .heavy, design: .rounded))
                                .foregroundStyle(FluoThemeTokens.liquidGradient)
                            Text(t("app.tagline"))
                                .font(.subheadline.weight(.semibold))
                                .padding(.horizontal, 12)
                                .padding(.vertical, 5)
                                .liquidGlassPill(tint: FluoThemeTokens.indigo)
                        }

                        Text(t("onb.desc"))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        LiquidGlassGroup(spacing: 12) {
                            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                                ForEach(AppLanguage.allCases) { lang in
                                    let selected = store.uiLang == lang
                                    Button {
                                        store.uiLang = lang
                                        store.targetLanguage = lang.defaultTargetLanguage
                                        store.savePreferences()
                                    } label: {
                                        HStack {
                                            Text(lang.nativeName).fontWeight(.bold)
                                            Spacer()
                                            Text(lang.shortCode)
                                                .font(.caption.bold())
                                                .foregroundStyle(selected ? .white : .secondary)
                                        }
                                        .padding(16)
                                        .foregroundStyle(selected ? .white : .primary)
                                        .contentShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())
                                    .liquidGlass(
                                        cornerRadius: 18,
                                        tint: selected ? FluoThemeTokens.indigo : .clear,
                                        strokeOpacity: selected ? 1.0 : 0.7,
                                        interactive: true,
                                        prominent: selected
                                    )
                                }
                            }
                        }

                        VStack(alignment: .leading, spacing: 8) {
                            Text(t("onb.targetLabel"))
                                .font(.footnote.weight(.bold))
                                .foregroundStyle(.secondary)
                            TextField(t("onb.targetLabel"), text: $store.targetLanguage)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 12)
                                .liquidGlass(cornerRadius: 16)
                        }

                        Button {
                            store.onboardingCompleted = true
                            store.savePreferences()
                            dismiss()
                        } label: {
                            Text(t("onb.startBtn"))
                                .font(.headline)
                                .foregroundStyle(.white)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 14)
                                .contentShape(Capsule())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true, prominent: true)
                    }
                    .padding(20)
                }
            }
            .navigationTitle(t("onb.title"))
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

struct TranslateSheet: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    var openSettings: () -> Void
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                ScrollView {
                    VStack(spacing: 14) {
                        if store.provider == .hyMt2 && !catalog.isInstalled(store.selectedLocalModel) {
                            VStack(alignment: .leading, spacing: 10) {
                                Text(t("tr.needModel"))
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                                let rec = ModelCatalog.specs[0]
                                if let pct = catalog.downloadProgress[rec.id] {
                                    ProgressView(value: pct).tint(FluoThemeTokens.indigo)
                                } else {
                                    Button {
                                        catalog.startDownload(rec)
                                    } label: {
                                        Text(t("tr.downloadRec"))
                                            .font(.subheadline.bold())
                                            .foregroundStyle(.white)
                                            .frame(maxWidth: .infinity)
                                            .padding(.vertical, 11)
                                            .contentShape(Capsule())
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())
                                    .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true, prominent: true)
                                }
                            }
                            .padding(16)
                            .liquidGlass(cornerRadius: 20, tint: FluoThemeTokens.indigo)
                        }

                        if let err = store.translationError {
                            Text(err)
                                .font(.footnote)
                                .foregroundStyle(.red)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(14)
                                .liquidGlass(cornerRadius: 16, tint: .red)
                        }

                        // 2x2 Control-Center-style Liquid Glass tiles
                        LiquidGlassGroup(spacing: 10) {
                            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                                glassActionTile(
                                    title: t("tr.inline"),
                                    icon: "sparkles",
                                    active: store.activeTab.displayMode == .inline
                                ) {
                                    dismiss()
                                    store.translateActiveTab(bilingual: false)
                                }

                                glassActionTile(
                                    title: t("tr.bilingual"),
                                    icon: "rectangle.split.2x1",
                                    active: store.activeTab.displayMode == .bilingual
                                ) {
                                    dismiss()
                                    store.translateActiveTab(bilingual: true)
                                }

                                glassActionTile(
                                    title: t("tr.regionPicker"),
                                    icon: "hand.tap",
                                    active: store.isPickingRegion
                                ) {
                                    dismiss()
                                    store.startRegionPicker(bilingual: false)
                                }

                                glassActionTile(
                                    title: t("tr.selection"),
                                    icon: "text.quote",
                                    active: false
                                ) {
                                    dismiss()
                                    store.translateSelectedText()
                                }
                            }
                        }

                        Button {
                            dismiss()
                            store.restoreOriginal()
                        } label: {
                            Label(t("tr.original"), systemImage: "arrow.uturn.backward")
                                .font(.subheadline.weight(.semibold))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 12)
                                .contentShape(Capsule())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .liquidGlassPill(interactive: true)

                        // Translation styles & typography card
                        VStack(alignment: .leading, spacing: 12) {
                            Text(t("tr.style"))
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)

                            ScrollView(.horizontal, showsIndicators: false) {
                                LiquidGlassGroup(spacing: 8) {
                                    HStack(spacing: 8) {
                                        ForEach(TranslationStyle.allCases) { st in
                                            let active = store.userData.style == st
                                            Button {
                                                store.userData.style = st
                                                store.savePreferences()
                                            } label: {
                                                Text(t("style.\(st.rawValue)"))
                                                    .font(.caption.weight(.semibold))
                                                    .padding(.horizontal, 12)
                                                    .padding(.vertical, 7)
                                                    .foregroundStyle(active ? .white : .primary)
                                                    .contentShape(Capsule())
                                            }
                                            .buttonStyle(LiquidPressButtonStyle())
                                            .liquidGlassPill(
                                                tint: active ? FluoThemeTokens.indigo : .clear,
                                                interactive: true,
                                                prominent: active
                                            )
                                        }
                                    }
                                }
                            }

                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text(t("tr.fontScale")).font(.caption.weight(.medium))
                                    Spacer()
                                    Text(String(format: "%.2fx", store.userData.fontScale)).font(.caption.monospacedDigit())
                                }
                                Slider(value: $store.userData.fontScale, in: 0.8...1.6, step: 0.05)
                                    .tint(FluoThemeTokens.indigo)
                                    .onChange(of: store.userData.fontScale) { _ in store.savePreferences() }
                            }

                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text(t("tr.lineHeight")).font(.caption.weight(.medium))
                                    Spacer()
                                    Text(String(format: "%.1f", store.userData.lineHeight)).font(.caption.monospacedDigit())
                                }
                                Slider(value: $store.userData.lineHeight, in: 1.2...2.2, step: 0.1)
                                    .tint(FluoThemeTokens.indigo)
                                    .onChange(of: store.userData.lineHeight) { _ in store.savePreferences() }
                            }

                            Button {
                                openSettings()
                            } label: {
                                HStack {
                                    Label("\(store.provider.rawValue) · \(store.targetLanguage)", systemImage: "slider.horizontal.3")
                                    Spacer()
                                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.secondary)
                                }
                                .font(.subheadline.weight(.medium))
                                .padding(.top, 4)
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(16)
                        .liquidGlass(cornerRadius: 22)
                    }
                    .padding(16)
                }
            }
            .navigationTitle(t("sheet.translateTitle"))
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    private func glassActionTile(title: String, icon: String, active: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 10) {
                Image(systemName: icon)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(active ? .white : FluoThemeTokens.indigo)
                Text(title)
                    .font(.caption.weight(.bold))
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .foregroundStyle(active ? .white : .primary)
            }
            .frame(maxWidth: .infinity, minHeight: 84, alignment: .topLeading)
            .padding(14)
            .contentShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
        .liquidGlass(
            cornerRadius: 20,
            tint: active ? FluoThemeTokens.indigo : .clear,
            interactive: true,
            prominent: active
        )
    }
}

struct TabsSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    @State private var showNewWorkspace = false
    @State private var wsDraftName = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                VStack(spacing: 10) {
                    ScrollView(.horizontal, showsIndicators: false) {
                        LiquidGlassGroup(spacing: 8) {
                            HStack(spacing: 8) {
                                ForEach(store.userData.workspaces) { ws in
                                    let active = store.userData.activeWorkspaceId == ws.id
                                    Button {
                                        store.switchWorkspace(ws.id)
                                    } label: {
                                        Text(ws.name)
                                            .font(.caption.weight(.bold))
                                            .padding(.horizontal, 14)
                                            .padding(.vertical, 8)
                                            .foregroundStyle(active ? .white : .primary)
                                            .contentShape(Capsule())
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())
                                    .liquidGlassPill(
                                        tint: active ? FluoThemeTokens.indigo : .clear,
                                        interactive: true,
                                        prominent: active
                                    )
                                    .contextMenu {
                                        if ws.id != "default" {
                                            Button(role: .destructive) {
                                                store.deleteWorkspace(ws.id)
                                            } label: {
                                                Label(t("set.delete"), systemImage: "trash")
                                            }
                                        }
                                    }
                                }

                                Button {
                                    wsDraftName = ""
                                    showNewWorkspace = true
                                } label: {
                                    Image(systemName: "plus")
                                        .font(.caption.bold())
                                        .frame(width: 30, height: 30)
                                        .contentShape(Circle())
                                }
                                .buttonStyle(LiquidPressButtonStyle())
                                .liquidGlassCircle(interactive: true)
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                    }

                    ScrollView {
                        LazyVStack(spacing: 10) {
                            ForEach(store.activeWorkspaceTabs) { tab in
                                SwipeableTabCard(store: store, tab: tab, dismiss: { dismiss() })
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.bottom, 20)
                    }
                }
            }
            .navigationTitle(t("tabs.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button {
                        store.newTab(isPrivate: true)
                        dismiss()
                    } label: {
                        Label(t("tabs.newPrivate"), systemImage: "eye.slash")
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button {
                        store.newTab(isPrivate: false)
                        dismiss()
                    } label: {
                        Label(t("tabs.newTab"), systemImage: "plus")
                    }
                }
            }
            .alert(t("ws.new"), isPresented: $showNewWorkspace) {
                TextField(t("ws.namePlaceholder"), text: $wsDraftName)
                Button(t("common.cancel"), role: .cancel) {}
                Button(t("common.add")) { store.createWorkspace(name: wsDraftName) }
            }
        }
    }
}

private struct SwipeableTabCard: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var tab: BrowserTab
    var dismiss: () -> Void

    @State private var dragX: CGFloat = 0

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var canSwipeClose: Bool {
        !tab.isPinned && store.tabs.count > 1
    }

    var body: some View {
        let isCurrent = tab.id == store.activeTabId
        ZStack(alignment: .trailing) {
            if dragX < -8 {
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .fill(.red.opacity(min(0.85, Double(-dragX / 95) * 0.85)))
                    .overlay(alignment: .trailing) {
                        Image(systemName: "trash.fill")
                            .foregroundStyle(.white)
                            .padding(.trailing, 20)
                            .opacity(min(1.0, Double(-dragX / 55)))
                    }
            }

            HStack(spacing: 10) {
                Button {
                    store.activeTabId = tab.id
                    dismiss()
                } label: {
                    HStack(spacing: 10) {
                        if tab.isPinned {
                            Image(systemName: "pin.fill")
                                .foregroundStyle(FluoThemeTokens.indigo)
                                .font(.caption)
                        }
                        if tab.isPrivate {
                            Text(t("tabs.privateBadge"))
                                .font(.caption2.bold())
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(FluoThemeTokens.violet.opacity(0.28), in: Capsule())
                        }
                        VStack(alignment: .leading, spacing: 3) {
                            Text(tab.title)
                                .font(.subheadline.weight(isCurrent ? .bold : .medium))
                                .lineLimit(1)
                            Text(tab.urlString)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)

                if canSwipeClose {
                    Button(role: .destructive) {
                        store.closeTab(tab.id)
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(14)
            .liquidGlass(
                cornerRadius: 18,
                tint: tab.isPrivate ? FluoThemeTokens.violet : (isCurrent ? FluoThemeTokens.indigo : .clear)
            )
            .contextMenu {
                Button {
                    store.togglePinTab(tab.id)
                } label: {
                    Label(tab.isPinned ? t("tabs.unpin") : t("tabs.pin"), systemImage: "pin")
                }

                Menu(t("tabs.moveWs")) {
                    ForEach(store.userData.workspaces) { ws in
                        Button(ws.name) { store.moveTab(tab.id, toWorkspace: ws.id) }
                    }
                }
            }
            .offset(x: dragX)
            .simultaneousGesture(
                DragGesture(minimumDistance: 20)
                    .onChanged { value in
                        guard canSwipeClose else { return }
                        let dx = value.translation.width
                        let dy = value.translation.height
                        if dx < 0 && abs(dx) > abs(dy) * 1.35 {
                            dragX = max(-140, dx)
                        }
                    }
                    .onEnded { value in
                        guard canSwipeClose else {
                            dragX = 0
                            return
                        }
                        let dx = value.translation.width
                        let dy = value.translation.height
                        if dx < -95 && abs(dx) > abs(dy) {
                            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                            withAnimation(.easeOut(duration: 0.18)) {
                                dragX = -320
                            }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.14) {
                                store.closeTab(tab.id)
                            }
                        } else {
                            withAnimation(.spring(response: 0.28, dampingFraction: 0.78)) {
                                dragX = 0
                            }
                        }
                    }
            )
        }
    }
}

struct MenuSheet: View {
    @ObservedObject var store: BrowserStore
    var openSheet: (ActiveSheetKind) -> Void
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                ScrollView {
                    VStack(spacing: 14) {
                        LiquidGlassGroup(spacing: 10) {
                            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                                quickTile(
                                    title: store.isCurrentPageBookmarked ? t("menu.bookmarkRemove") : t("menu.bookmarkAdd"),
                                    icon: store.isCurrentPageBookmarked ? "star.fill" : "star",
                                    active: store.isCurrentPageBookmarked
                                ) {
                                    store.toggleCurrentBookmark()
                                    dismiss()
                                }

                                quickTile(
                                    title: t("menu.find"),
                                    icon: "magnifyingglass",
                                    active: store.showFindBar
                                ) {
                                    dismiss()
                                    store.showFindBar = true
                                }

                                quickTile(
                                    title: t("menu.desktop"),
                                    icon: "desktopcomputer",
                                    active: store.activeTab.isDesktopMode
                                ) {
                                    store.activeTab.applyDesktopMode(!store.activeTab.isDesktopMode)
                                    dismiss()
                                }

                                quickTile(
                                    title: store.currentHost.isEmpty ? t("menu.adblockSite") : store.currentHost,
                                    icon: "shield.lefthalf.filled",
                                    active: store.isCurrentHostAdBlockEnabled
                                ) {
                                    if !store.currentHost.isEmpty {
                                        store.toggleCurrentHostAdBlock()
                                    }
                                    dismiss()
                                }
                            }
                        }

                        VStack(spacing: 0) {
                            menuRow(t("menu.bookmarks"), icon: "book") { openSheet(.bookmarks) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.history"), icon: "clock") { openSheet(.history) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.trHistory"), icon: "clock.arrow.2.circlepath") { openSheet(.translationHistory) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.glossary"), icon: "character.book.closed") { openSheet(.glossary) }
                        }
                        .liquidGlass(cornerRadius: 22)

                        VStack(spacing: 0) {
                            menuRow(t("menu.feedback"), icon: "ladybug") { openSheet(.feedback) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.settings"), icon: "gearshape") { openSheet(.settings) }
                        }
                        .liquidGlass(cornerRadius: 22)
                    }
                    .padding(16)
                }
            }
            .navigationTitle(t("menu.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) { dismiss() }
                }
            }
        }
    }

    private func quickTile(title: String, icon: String, active: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: icon)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(active ? .white : FluoThemeTokens.indigo)
                Text(title)
                    .font(.caption.weight(.bold))
                    .lineLimit(1)
                    .foregroundStyle(active ? .white : .primary)
            }
            .frame(maxWidth: .infinity, minHeight: 74, alignment: .topLeading)
            .padding(14)
            .contentShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
        .liquidGlass(
            cornerRadius: 20,
            tint: active ? FluoThemeTokens.indigo : .clear,
            interactive: true,
            prominent: active
        )
    }

    private func menuRow(_ title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack {
                Label(title, systemImage: icon)
                    .font(.subheadline.weight(.medium))
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

struct BookmarksSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss
    @State private var search = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var filtered: [BookmarkItem] {
        let q = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty { return store.userData.bookmarks }
        return store.userData.bookmarks.filter {
            $0.title.localizedCaseInsensitiveContains(q) || $0.url.localizedCaseInsensitiveContains(q)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                if filtered.isEmpty {
                    Text(t("bm.empty")).foregroundStyle(.secondary)
                        .listRowBackground(Color.white.opacity(0.06))
                } else {
                    ForEach(filtered) { item in
                        Button {
                            store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                            dismiss()
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.title).font(.subheadline.weight(.medium)).lineLimit(1)
                                Text(item.url).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        .listRowBackground(Color.white.opacity(0.08))
                        .swipeActions {
                            Button(role: .destructive) { store.deleteBookmark(item.id) } label: { Text(t("set.delete")) }
                        }
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .searchable(text: $search, prompt: t("bm.search"))
            .navigationTitle(t("bm.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct HistorySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                if store.userData.history.isEmpty {
                    Text(t("hist.empty")).foregroundStyle(.secondary)
                        .listRowBackground(Color.white.opacity(0.06))
                } else {
                    ForEach(store.userData.history) { entry in
                        Button {
                            store.activeTab.navigate(input: entry.url, searchEngine: store.userData.searchEngine)
                            dismiss()
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(entry.title).font(.subheadline).lineLimit(1)
                                Text(entry.url).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        .listRowBackground(Color.white.opacity(0.08))
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("hist.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    if !store.userData.history.isEmpty {
                        Button(t("hist.clear"), role: .destructive) { store.clearHistory() }
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct TranslationHistorySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                if store.userData.translationHistory.isEmpty {
                    Text(t("trhist.empty")).foregroundStyle(.secondary)
                        .listRowBackground(Color.white.opacity(0.06))
                } else {
                    ForEach(store.userData.translationHistory) { item in
                        Button {
                            store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                            dismiss()
                        } label: {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(item.title).font(.subheadline.weight(.medium)).lineLimit(1)
                                Text("\(item.targetLanguage) · \(item.provider)")
                                    .font(.caption).foregroundStyle(FluoThemeTokens.indigo)
                                Text(item.url).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        .listRowBackground(Color.white.opacity(0.08))
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("trhist.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct GlossarySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    @State private var src = ""
    @State private var dst = ""
    @State private var caseSensitive = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(t("gl.source"), text: $src)
                    TextField(t("gl.target"), text: $dst)
                    Toggle(t("gl.case"), isOn: $caseSensitive)
                    Button(t("gl.add")) {
                        store.addGlossaryItem(source: src, target: dst, caseSensitive: caseSensitive)
                        src = ""
                        dst = ""
                    }
                    .disabled(src.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                .listRowBackground(Color.white.opacity(0.08))

                Section {
                    if store.userData.glossary.isEmpty {
                        Text(t("gl.empty")).foregroundStyle(.secondary)
                    } else {
                        ForEach(store.userData.glossary) { item in
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(item.source).font(.subheadline.weight(.semibold))
                                    Text(item.target.isEmpty ? "(keep original)" : item.target)
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button(role: .destructive) {
                                    store.deleteGlossaryItem(item.id)
                                } label: {
                                    Image(systemName: "trash")
                                }
                            }
                        }
                    }
                }
                .listRowBackground(Color.white.opacity(0.08))
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("gl.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct SelectionResultSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text(store.selectionSourceText).font(.subheadline).foregroundStyle(.secondary)
                }
                .listRowBackground(Color.white.opacity(0.08))

                Section {
                    if store.selectionTranslatedText.isEmpty {
                        HStack(spacing: 8) {
                            ProgressView()
                            Text(t("tr.progress"))
                        }
                    } else {
                        Text(store.selectionTranslatedText).font(.body)
                        Button(t("sel.copy")) {
                            UIPasteboard.general.string = store.selectionTranslatedText
                        }
                        Button(t("sel.addGlossary")) {
                            store.addGlossaryItem(source: store.selectionSourceText, target: store.selectionTranslatedText, caseSensitive: false)
                            dismiss()
                        }
                    }
                }
                .listRowBackground(Color.white.opacity(0.08))
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("sel.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct FeedbackSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    @State private var category = "bug"
    @State private var summary = ""
    @State private var details = ""
    @State private var includeDiag = true

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var markdownReport: String {
        var lines = [
            "## Summary",
            summary.trimmingCharacters(in: .whitespacesAndNewlines),
            "",
            "## Details",
            details.trimmingCharacters(in: .whitespacesAndNewlines),
        ]
        if includeDiag {
            lines.append("")
            lines.append("## iOS Diagnostics (Sanitized)")
            lines.append("- Platform: Fluo iOS \(UIDevice.current.systemVersion)")
            lines.append("- UI / Target: \(store.uiLang.rawValue) -> \(store.targetLanguage)")
            lines.append("- Provider: \(store.provider.rawValue)")
            lines.append("- Style: \(store.userData.style.rawValue)")
        }
        return lines.joined(separator: "\n")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker(t("fb.category"), selection: $category) {
                        Text(t("fb.bug")).tag("bug")
                        Text(t("fb.ux")).tag("enhancement")
                        Text(t("fb.mistranslation")).tag("translation-error")
                    }
                    TextField(t("fb.summary"), text: $summary)
                    TextEditor(text: $details).frame(minHeight: 110)
                    Toggle(t("fb.includeDiag"), isOn: $includeDiag)
                }
                .listRowBackground(Color.white.opacity(0.08))

                Section {
                    Button(t("fb.openGithub")) {
                        let title = summary.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "Fluo%20iOS%20Feedback"
                        let body = markdownReport.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
                        let label = category.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "bug"
                        if let url = URL(string: "https://github.com/apple-out/transratebrowser/issues/new?title=\(title)&labels=\(label)&body=\(body)") {
                            openURL(url)
                        }
                    }
                    Button(t("fb.copyMd")) {
                        UIPasteboard.general.string = markdownReport
                    }
                }
                .listRowBackground(Color.white.opacity(0.08))
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("fb.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) { dismiss() }
                }
            }
        }
    }
}

struct SettingsSheet: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    @Environment(\.dismiss) private var dismiss

    @State private var geminiKeyDraft = KeychainStore.load(account: "gemini_api_key") ?? ""
    @State private var openAiKeyDraft = KeychainStore.load(account: "openai_api_key") ?? ""
    @State private var showClearedAlert = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            Form {
                Section(t("set.langSection")) {
                    Picker(t("set.uiLang"), selection: $store.uiLang) {
                        ForEach(AppLanguage.allCases) { lang in
                            Text(lang.nativeName).tag(lang)
                        }
                    }
                    .onChange(of: store.uiLang) { newLang in
                        store.targetLanguage = newLang.defaultTargetLanguage
                        store.savePreferences()
                    }

                    TextField(t("set.targetLang"), text: $store.targetLanguage)
                        .onChange(of: store.targetLanguage) { _ in store.savePreferences() }
                }
                .listRowBackground(Color.white.opacity(0.08))

                Section(t("set.browserSection")) {
                    Picker(t("set.searchEngine"), selection: $store.userData.searchEngine) {
                        ForEach(SearchEngine.allCases) { engine in
                            Text(engine.displayName).tag(engine)
                        }
                    }
                    .onChange(of: store.userData.searchEngine) { _ in store.savePreferences() }

                    Picker(t("set.theme"), selection: $store.userData.appTheme) {
                        Text(t("theme.system")).tag(AppTheme.system)
                        Text(t("theme.light")).tag(AppTheme.light)
                        Text(t("theme.dark")).tag(AppTheme.dark)
                    }
                    .onChange(of: store.userData.appTheme) { _ in store.savePreferences() }

                    Toggle(t("set.adblock"), isOn: $store.userData.adBlockEnabled)
                        .onChange(of: store.userData.adBlockEnabled) { enabled in
                            store.savePreferences()
                            AdBlockManager.shared.apply(
                                to: store.activeTab.webView,
                                enabled: enabled,
                                host: store.currentHost,
                                allowlist: store.userData.adBlockAllowlist
                            )
                        }

                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(t("set.zoom"))
                            Spacer()
                            Text("\(Int((store.activeTab.pageZoom * 100).rounded()))%").foregroundStyle(.secondary)
                        }
                        Slider(value: Binding(
                            get: { store.activeTab.pageZoom },
                            set: { store.activeTab.applyPageZoom($0) }
                        ), in: 0.7...1.8, step: 0.1)
                        .tint(FluoThemeTokens.indigo)
                    }

                    Button(t("set.clearSiteData"), role: .destructive) {
                        Task {
                            await store.clearSiteData()
                            showClearedAlert = true
                        }
                    }
                }
                .listRowBackground(Color.white.opacity(0.08))

                Section(t("set.aiSection")) {
                    Picker(t("set.provider"), selection: $store.provider) {
                        Text(t("set.provider.local")).tag(AiProviderKind.hyMt2)
                        Text(t("set.provider.gemini")).tag(AiProviderKind.googleGemini)
                        Text(t("set.provider.openai")).tag(AiProviderKind.openAiCompatible)
                    }
                    .onChange(of: store.provider) { _ in store.savePreferences() }
                }
                .listRowBackground(Color.white.opacity(0.08))

                if store.provider == .hyMt2 {
                    Section {
                        Text(t("set.modelsHint"))
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        ForEach(ModelCatalog.specs) { spec in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(spec.displayName).font(.subheadline.weight(.semibold))
                                Text(spec.subtitle).font(.caption).foregroundStyle(.secondary)

                                if let pct = catalog.downloadProgress[spec.id] {
                                    HStack {
                                        ProgressView(value: pct).tint(FluoThemeTokens.indigo)
                                        Button(t("set.cancel")) { catalog.cancelDownload(spec.id) }
                                            .font(.caption)
                                    }
                                } else if catalog.isInstalled(spec.id) {
                                    HStack {
                                        Button {
                                            store.selectedLocalModel = spec.id
                                            store.savePreferences()
                                        } label: {
                                            Label(
                                                t("set.installed"),
                                                systemImage: store.selectedLocalModel == spec.id ? "checkmark.circle.fill" : "circle"
                                            )
                                        }
                                        Spacer()
                                        Button(t("set.delete"), role: .destructive) {
                                            catalog.deleteModel(spec.id)
                                        }
                                        .font(.caption)
                                    }
                                } else {
                                    Button(t("set.download")) {
                                        catalog.startDownload(spec)
                                    }
                                    .buttonStyle(.borderedProminent)
                                    .tint(FluoThemeTokens.indigo)
                                }
                            }
                            .padding(.vertical, 4)
                        }

                        if TranslationEngine.shared.activeLocalModelPath != nil {
                            Button(t("set.unload"), role: .destructive) {
                                TranslationEngine.shared.unloadOnDeviceModel()
                            }
                        }

                        if let err = catalog.statusError {
                            Text(err).font(.caption).foregroundStyle(.red)
                        }
                    } header: {
                        Text(t("set.modelsSection"))
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                } else if store.provider == .googleGemini {
                    Section(t("set.provider.gemini")) {
                        SecureField(t("set.apiKey"), text: $geminiKeyDraft)
                        TextField(t("set.modelId"), text: $store.geminiModel)
                            .onChange(of: store.geminiModel) { _ in store.savePreferences() }
                        HStack {
                            Button(t("set.saveKey")) {
                                KeychainStore.save(account: "gemini_api_key", value: geminiKeyDraft)
                            }
                            Spacer()
                            Button(t("set.clearKey"), role: .destructive) {
                                geminiKeyDraft = ""
                                KeychainStore.delete(account: "gemini_api_key")
                            }
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                } else {
                    Section(t("set.provider.openai")) {
                        TextField(t("set.baseUrl"), text: $store.openAiBaseUrl)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .onChange(of: store.openAiBaseUrl) { _ in store.savePreferences() }
                        SecureField(t("set.apiKey"), text: $openAiKeyDraft)
                        TextField(t("set.modelId"), text: $store.openAiModel)
                            .textInputAutocapitalization(.never)
                            .onChange(of: store.openAiModel) { _ in store.savePreferences() }
                        HStack {
                            Button(t("set.saveKey")) {
                                KeychainStore.save(account: "openai_api_key", value: openAiKeyDraft)
                            }
                            Spacer()
                            Button(t("set.clearKey"), role: .destructive) {
                                openAiKeyDraft = ""
                                KeychainStore.delete(account: "openai_api_key")
                            }
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("set.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) {
                        store.savePreferences()
                        dismiss()
                    }
                }
            }
            .alert(t("set.cleared"), isPresented: $showClearedAlert) {
                Button(t("common.done"), role: .cancel) {}
            }
        }
    }
}
