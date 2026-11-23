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
                            store.summarizeActivePage()
                        } label: {
                            Label(t("tr.aiSummary"), systemImage: "sparkles.rectangle.stack.fill")
                                .font(.subheadline.weight(.semibold))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 12)
                                .contentShape(Capsule())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true)

                        HStack(spacing: 10) {
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

                            Button {
                                dismiss()
                                store.startOrToggleTTS()
                            } label: {
                                Label(t("tr.tts"), systemImage: store.isSpeakingTTS ? "speaker.wave.3.fill" : "speaker.wave.2")
                                    .font(.subheadline.weight(.semibold))
                                    .foregroundStyle(store.isSpeakingTTS ? .white : .primary)
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 12)
                                    .contentShape(Capsule())
                            }
                            .buttonStyle(LiquidPressButtonStyle())
                            .liquidGlassPill(
                                tint: store.isSpeakingTTS ? FluoThemeTokens.indigo : .clear,
                                interactive: true,
                                prominent: store.isSpeakingTTS
                            )
                        }

                        if !store.currentHost.isEmpty {
                            HStack {
                                Label(t("tr.alwaysHost"), systemImage: "globe.badge.chevron.backward")
                                    .font(.caption.weight(.semibold))
                                Spacer()
                                Toggle("", isOn: Binding(
                                    get: { store.isCurrentHostAlwaysTranslated },
                                    set: { _ in store.toggleAlwaysTranslateCurrentHost() }
                                ))
                                .labelsHidden()
                                .tint(FluoThemeTokens.indigo)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 10)
                            .liquidGlass(cornerRadius: 18)
                        }

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
    @State private var showRenameWorkspace = false
    @State private var wsTargetId = ""
    @State private var wsDraftName = ""
    @State private var tabSearch = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var activeWorkspace: WorkspaceItem? {
        store.userData.workspaces.first(where: { $0.id == store.userData.activeWorkspaceId })
    }

    private var filteredTabs: [BrowserTab] {
        let q = tabSearch.trimmingCharacters(in: .whitespacesAndNewlines)
        let list = store.activeWorkspaceTabs
        if q.isEmpty { return list }
        return list.filter {
            $0.title.localizedCaseInsensitiveContains(q) || $0.urlString.localizedCaseInsensitiveContains(q)
        }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                VStack(spacing: 10) {
                    HStack(spacing: 8) {
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
                                        .contentShape(.contextMenuPreview, Capsule())
                                        .contextMenu {
                                            Button {
                                                wsTargetId = ws.id
                                                wsDraftName = ws.name
                                                showRenameWorkspace = true
                                            } label: {
                                                Label(t("ws.rename"), systemImage: "pencil")
                                            }
                                            if ws.id != "default" {
                                                Button(role: .destructive) {
                                                    store.deleteWorkspace(ws.id)
                                                } label: {
                                                    Label(t("ws.delete"), systemImage: "trash")
                                                }
                                            }
                                        }
                                    }
                                }
                                .padding(.vertical, 6)
                                .padding(.horizontal, 2)
                            }
                        }
                        .fixedSize(horizontal: false, vertical: true)

                        Button {
                            wsDraftName = ""
                            showNewWorkspace = true
                        } label: {
                            Image(systemName: "plus")
                                .font(.caption.bold())
                                .frame(width: 32, height: 32)
                                .contentShape(Circle())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .liquidGlassCircle(interactive: true)

                        if let ws = activeWorkspace {
                            Menu {
                                Button {
                                    wsTargetId = ws.id
                                    wsDraftName = ws.name
                                    showRenameWorkspace = true
                                } label: {
                                    Label("\(t("ws.rename")): \(ws.name)", systemImage: "pencil")
                                }
                                if ws.id != "default" {
                                    Button(role: .destructive) {
                                        store.deleteWorkspace(ws.id)
                                    } label: {
                                        Label("\(t("ws.delete")): \(ws.name)", systemImage: "trash")
                                    }
                                }
                                let deletableOthers = store.userData.workspaces.filter { $0.id != "default" && $0.id != ws.id }
                                if !deletableOthers.isEmpty {
                                    Divider()
                                    ForEach(deletableOthers) { other in
                                        Button(role: .destructive) {
                                            store.deleteWorkspace(other.id)
                                        } label: {
                                            Label("\(t("ws.delete")): \(other.name)", systemImage: "trash")
                                        }
                                    }
                                }
                            } label: {
                                Image(systemName: "ellipsis")
                                    .font(.caption.bold())
                                    .frame(width: 32, height: 32)
                                    .contentShape(Circle())
                            }
                            .liquidGlassCircle(interactive: true)
                            .accessibilityLabel(t("ws.manage"))
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 8)

                    List {
                        ForEach(filteredTabs) { tab in
                            let isCurrent = tab.id == store.activeTabId
                            HStack(spacing: 10) {
                                Button {
                                    store.selectTab(tab.id)
                                    dismiss()
                                } label: {
                                    HStack(spacing: 10) {
                                        if tab.isPinned {
                                            Image(systemName: "pin.fill")
                                                .foregroundStyle(FluoThemeTokens.indigo)
                                                .font(.caption)
                                        }
                                        if tab.isSleeping {
                                            Text(t("tabs.sleepingBadge"))
                                                .font(.caption2.bold())
                                                .padding(.horizontal, 6)
                                                .padding(.vertical, 2)
                                                .background(Color.secondary.opacity(0.22), in: Capsule())
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
                                            Text(tab.isStartPage ? t("start.title") : tab.urlString)
                                                .font(.caption2)
                                                .foregroundStyle(.secondary)
                                                .lineLimit(1)
                                        }
                                    }
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)

                                if !tab.isPinned && (store.tabs.count > 1 || !tab.isStartPage) {
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
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 5, leading: 16, bottom: 5, trailing: 16))
                            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                if !tab.isPinned && (store.tabs.count > 1 || !tab.isStartPage) {
                                    Button(role: .destructive) {
                                        store.closeTab(tab.id)
                                    } label: {
                                        Label(t("set.delete"), systemImage: "trash")
                                    }
                                }
                            }
                            .swipeActions(edge: .leading, allowsFullSwipe: false) {
                                Button {
                                    store.togglePinTab(tab.id)
                                } label: {
                                    Label(tab.isPinned ? t("tabs.unpin") : t("tabs.pin"), systemImage: tab.isPinned ? "pin.slash" : "pin")
                                }
                                .tint(FluoThemeTokens.indigo)

                                Button {
                                    store.duplicateTab(tab.id)
                                } label: {
                                    Label(t("tabs.duplicate"), systemImage: "plus.square.on.square")
                                }
                                .tint(FluoThemeTokens.cyan)
                            }
                            .contextMenu {
                                Button {
                                    store.togglePinTab(tab.id)
                                } label: {
                                    Label(tab.isPinned ? t("tabs.unpin") : t("tabs.pin"), systemImage: "pin")
                                }

                                Button {
                                    store.duplicateTab(tab.id)
                                } label: {
                                    Label(t("tabs.duplicate"), systemImage: "plus.square.on.square")
                                }

                                if !tab.isStartPage && tab.id != store.activeTabId && !tab.isSleeping {
                                    Button {
                                        tab.putToSleep()
                                    } label: {
                                        Label(t("tabs.sleep"), systemImage: "moon.zzz")
                                    }
                                }

                                Menu(t("tabs.moveWs")) {
                                    ForEach(store.userData.workspaces) { ws in
                                        Button(ws.name) { store.moveTab(tab.id, toWorkspace: ws.id) }
                                    }
                                }

                                if !tab.isPinned && (store.tabs.count > 1 || !tab.isStartPage) {
                                    Button(role: .destructive) {
                                        store.closeTab(tab.id)
                                    } label: {
                                        Label(t("set.delete"), systemImage: "xmark.circle")
                                    }
                                }
                            }
                        }
                        .onMove { from, to in
                            if tabSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                store.moveTabsInCurrentWorkspace(fromOffsets: from, toOffset: to)
                            }
                        }
                    }
                    .listStyle(.plain)
                    .scrollContentBackground(.hidden)
                    .searchable(text: $tabSearch, prompt: t("tabs.search"))
                }
            }
            .navigationTitle(t("tabs.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItemGroup(placement: .navigationBarLeading) {
                    Button {
                        store.newTab(isPrivate: true)
                        dismiss()
                    } label: {
                        Label(t("tabs.newPrivate"), systemImage: "eye.slash")
                    }
                    if !store.closedTabs.isEmpty {
                        Button {
                            store.reopenClosedTab()
                            dismiss()
                        } label: {
                            Label(t("tabs.reopen"), systemImage: "arrow.uturn.left.circle")
                        }
                    }
                }
                ToolbarItemGroup(placement: .navigationBarTrailing) {
                    Menu {
                        Button {
                            store.deduplicateTabsInCurrentWorkspace()
                        } label: {
                            Label(t("tabs.deduplicate"), systemImage: "sparkles.rectangle.stack")
                        }
                        Button(role: .destructive) {
                            store.closeAllTabsInCurrentWorkspace()
                        } label: {
                            Label(t("tabs.closeAll"), systemImage: "trash")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
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
            .alert(t("ws.rename"), isPresented: $showRenameWorkspace) {
                TextField(t("ws.namePlaceholder"), text: $wsDraftName)
                Button(t("common.cancel"), role: .cancel) {}
                Button(t("common.save")) { store.renameWorkspace(wsTargetId, name: wsDraftName) }
            }
        }
    }
}

struct MenuSheet: View {
    @ObservedObject var store: BrowserStore
    var openSheet: (ActiveSheetKind) -> Void
    @Environment(\.dismiss) private var dismiss

    @State private var copiedFeedback = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                ScrollView {
                    VStack(spacing: 14) {
                        if !store.activeTab.isStartPage {
                            LiquidGlassGroup(spacing: 10) {
                                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                                    quickTile(
                                        title: store.isCurrentPageBookmarked ? t("bm.editTitle") : t("menu.bookmarkAdd"),
                                        icon: store.isCurrentPageBookmarked ? "star.fill" : "star",
                                        active: store.isCurrentPageBookmarked
                                    ) {
                                        openSheet(.bookmarkEditor)
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
                                if let shareUrl = URL(string: BrowserStore.cleanURLWithoutTrackers(store.activeTab.urlString)) {
                                    ShareLink(item: shareUrl) {
                                        HStack {
                                            Label(t("menu.share"), systemImage: "square.and.arrow.up")
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
                                    Divider().opacity(0.25)
                                }

                                menuRow(copiedFeedback ? t("menu.copied") : t("menu.cleanUrl"), icon: "doc.on.doc") {
                                    UIPasteboard.general.string = BrowserStore.cleanURLWithoutTrackers(store.activeTab.urlString)
                                    copiedFeedback = true
                                }
                                Divider().opacity(0.25)

                                menuRow(t("menu.readingListAdd"), icon: "eyeglasses") {
                                    store.addCurrentPageToReadingList()
                                    copiedFeedback = true
                                }
                                Divider().opacity(0.25)

                                menuRow(t("tr.tts"), icon: store.isSpeakingTTS ? "speaker.wave.3.fill" : "speaker.wave.2") {
                                    dismiss()
                                    store.startOrToggleTTS()
                                }
                                Divider().opacity(0.25)

                                menuRow(t("menu.readerMode"), icon: "doc.text.magnifyingglass") {
                                    store.toggleReaderMode()
                                    dismiss()
                                }
                                Divider().opacity(0.25)

                                menuRow(t("menu.smartDark"), icon: "moon.stars") {
                                    store.toggleSmartDarkPage()
                                    dismiss()
                                }
                                Divider().opacity(0.25)

                                menuRow(t("menu.printPdf"), icon: "printer") {
                                    dismiss()
                                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                                        store.printCurrentPage()
                                    }
                                }

                                if store.activeTab.displayMode != .original {
                                    Divider().opacity(0.25)
                                    menuRow(t("menu.exportMd"), icon: "doc.richtext") {
                                        Task {
                                            let md = await store.exportCurrentPageMarkdown()
                                            UIPasteboard.general.string = md
                                            copiedFeedback = true
                                        }
                                    }
                                }

                                Divider().opacity(0.25)
                                HStack {
                                    Label(t("set.zoom"), systemImage: "textformat.size")
                                        .font(.subheadline.weight(.medium))
                                    Spacer()
                                    Button {
                                        store.activeTab.applyPageZoom(store.activeTab.pageZoom - 0.1)
                                    } label: {
                                        Image(systemName: "minus.circle")
                                            .font(.title3)
                                    }
                                    .buttonStyle(.plain)
                                    Text("\(Int((store.activeTab.pageZoom * 100).rounded()))%")
                                        .font(.caption.monospacedDigit().weight(.bold))
                                        .frame(minWidth: 44)
                                        .onTapGesture {
                                            store.activeTab.applyPageZoom(1.0)
                                        }
                                    Button {
                                        store.activeTab.applyPageZoom(store.activeTab.pageZoom + 0.1)
                                    } label: {
                                        Image(systemName: "plus.circle")
                                            .font(.title3)
                                    }
                                    .buttonStyle(.plain)
                                }
                                .padding(.horizontal, 16)
                                .padding(.vertical, 11)
                            }
                            .liquidGlass(cornerRadius: 22)
                        }

                        if !store.activeTab.isStartPage || !store.closedTabs.isEmpty || store.activeWorkspaceTabs.count > 1 {
                            VStack(spacing: 0) {
                                if !store.activeTab.isStartPage {
                                    menuRow(t("menu.startPage"), icon: "house") {
                                        store.activeTab.openStartPage()
                                        dismiss()
                                    }
                                }
                                if store.activeWorkspaceTabs.contains(where: { !$0.isStartPage }) {
                                    if !store.activeTab.isStartPage { Divider().opacity(0.25) }
                                    menuRow(t("menu.bookmarkAll"), icon: "star.square.on.square") {
                                        store.bookmarkAllTabs()
                                        dismiss()
                                    }
                                }
                                if !store.closedTabs.isEmpty {
                                    if !store.activeTab.isStartPage || store.activeWorkspaceTabs.contains(where: { !$0.isStartPage }) {
                                        Divider().opacity(0.25)
                                    }
                                    menuRow(t("tabs.reopen"), icon: "arrow.uturn.left.circle") {
                                        store.reopenClosedTab()
                                        dismiss()
                                    }
                                }
                            }
                            .liquidGlass(cornerRadius: 22)
                        }

                        VStack(spacing: 0) {
                            menuRow(t("menu.bookmarks"), icon: "book") { openSheet(.bookmarks) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.history"), icon: "clock") { openSheet(.history) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.trHistory"), icon: "clock.arrow.2.circlepath") { openSheet(.translationHistory) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.glossary"), icon: "character.book.closed") { openSheet(.glossary) }
                            Divider().opacity(0.25)
                            menuRow(t("menu.downloads"), icon: "arrow.down.circle") { openSheet(.downloads) }
                        }
                        .liquidGlass(cornerRadius: 22)

                        VStack(spacing: 0) {
                            if !store.activeTab.isStartPage {
                                menuRow(t("sec.title"), icon: "lock.shield") { openSheet(.siteSecurity) }
                                Divider().opacity(0.25)
                            }
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
    var onEdit: (BookmarkItem) -> Void
    var onAddCurrent: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss
    @State private var search = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var filtered: [BookmarkItem] {
        let q = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty { return store.userData.bookmarks }
        return store.userData.bookmarks.filter {
            $0.title.localizedCaseInsensitiveContains(q)
                || $0.url.localizedCaseInsensitiveContains(q)
                || $0.folder.localizedCaseInsensitiveContains(q)
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
                        HStack(spacing: 10) {
                            Button {
                                store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                                dismiss()
                            } label: {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(item.title).font(.subheadline.weight(.medium)).lineLimit(1)
                                    HStack(spacing: 6) {
                                        if !item.folder.isEmpty {
                                            Text(item.folder)
                                                .font(.caption2.bold())
                                                .foregroundStyle(FluoThemeTokens.indigo)
                                        }
                                        Text(item.url).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                    }
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .buttonStyle(.plain)

                            Button {
                                onEdit(item)
                            } label: {
                                Image(systemName: "pencil.circle")
                                    .foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)
                        }
                        .listRowBackground(Color.white.opacity(0.08))
                        .swipeActions {
                            Button(role: .destructive) { store.deleteBookmark(item.id) } label: { Text(t("set.delete")) }
                            Button { onEdit(item) } label: { Text(t("bm.editTitle")) }.tint(FluoThemeTokens.indigo)
                        }
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .searchable(text: $search, prompt: t("bm.search"))
            .navigationTitle(t("bm.title"))
            .toolbar {
                if let onAddCurrent {
                    ToolbarItem(placement: .navigationBarLeading) {
                        Button {
                            onAddCurrent()
                        } label: {
                            Label(store.isCurrentPageBookmarked ? t("bm.editTitle") : t("bm.addTitle"), systemImage: "plus")
                        }
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct BookmarkEditorSheet: View {
    @ObservedObject var store: BrowserStore
    var existing: BookmarkItem?
    @Environment(\.dismiss) private var dismiss

    @State private var nameDraft = ""
    @State private var urlDraft = ""
    @State private var folderDraft = "Default"

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(t("bm.nameLabel"), text: $nameDraft)
                    TextField(t("bm.urlLabel"), text: $urlDraft)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    TextField(t("bm.folderLabel"), text: $folderDraft)
                }
                .listRowBackground(Color.white.opacity(0.08))

                if let existing {
                    Section {
                        Button(t("bm.remove"), role: .destructive) {
                            store.deleteBookmark(existing.id)
                            dismiss()
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(existing == nil ? t("bm.addTitle") : t("bm.editTitle"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button(t("common.cancel")) { dismiss() }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("bm.save")) {
                        store.upsertBookmark(
                            id: existing?.id,
                            title: nameDraft,
                            url: urlDraft,
                            folder: folderDraft
                        )
                        dismiss()
                    }
                    .disabled(urlDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .onAppear {
                if let existing {
                    nameDraft = existing.title
                    urlDraft = existing.url
                    folderDraft = existing.folder
                } else {
                    nameDraft = store.activeTab.title
                    urlDraft = store.activeTab.urlString
                    folderDraft = "Default"
                }
            }
        }
    }
}

struct SiteSecuritySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss
    @State private var copied = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var isHttps: Bool {
        store.activeTab.urlString.lowercased().hasPrefix("https://")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    HStack(spacing: 12) {
                        Image(systemName: store.activeTab.isStartPage ? "sparkles" : (isHttps ? "lock.shield.fill" : "exclamationmark.triangle.fill"))
                            .font(.title2)
                            .foregroundStyle(store.activeTab.isStartPage ? FluoThemeTokens.indigo : (isHttps ? .green : .orange))
                        VStack(alignment: .leading, spacing: 3) {
                            Text(store.activeTab.isStartPage ? t("sec.internal") : (isHttps ? t("sec.https") : t("sec.http")))
                                .font(.subheadline.weight(.bold))
                            if !store.currentHost.isEmpty {
                                Text(store.currentHost)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }

                    if !store.activeTab.isStartPage {
                        Text(store.activeTab.urlString)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(3)

                        Button(copied ? t("menu.copied") : t("sec.copyUrl")) {
                            UIPasteboard.general.string = store.activeTab.urlString
                            copied = true
                        }
                    }
                }
                .listRowBackground(Color.white.opacity(0.08))

                if !store.activeTab.isStartPage {
                    Section {
                        if !store.currentHost.isEmpty {
                            Toggle(t("menu.adblockSite"), isOn: Binding(
                                get: { store.isCurrentHostAdBlockEnabled },
                                set: { _ in store.toggleCurrentHostAdBlock() }
                            ))

                            Toggle(t("tr.alwaysHost"), isOn: Binding(
                                get: { store.isCurrentHostAlwaysTranslated },
                                set: { _ in store.toggleAlwaysTranslateCurrentHost() }
                            ))
                        }

                        Toggle(t("menu.desktop"), isOn: Binding(
                            get: { store.activeTab.isDesktopMode },
                            set: { store.activeTab.applyDesktopMode($0) }
                        ))

                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(t("set.zoom"))
                                Spacer()
                                Text("\(Int((store.activeTab.pageZoom * 100).rounded()))%")
                                    .foregroundStyle(.secondary)
                            }
                            Slider(value: Binding(
                                get: { store.activeTab.pageZoom },
                                set: { store.activeTab.applyPageZoom($0) }
                            ), in: 0.7...1.8, step: 0.1)
                            .tint(FluoThemeTokens.indigo)
                        }

                        if !store.currentHost.isEmpty {
                            Button(t("sec.clearHostData"), role: .destructive) {
                                Task {
                                    await store.clearCurrentHostSiteData()
                                    dismiss()
                                }
                            }
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("sec.title"))
            .navigationBarTitleDisplayMode(.inline)
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
    @State private var search = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var filtered: [HistoryEntry] {
        let q = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty { return store.userData.history }
        return store.userData.history.filter {
            $0.title.localizedCaseInsensitiveContains(q) || $0.url.localizedCaseInsensitiveContains(q)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                if filtered.isEmpty {
                    Text(t("hist.empty")).foregroundStyle(.secondary)
                        .listRowBackground(Color.white.opacity(0.06))
                } else {
                    ForEach(filtered) { entry in
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
                        .swipeActions {
                            Button(role: .destructive) {
                                store.deleteHistoryItem(entry.id)
                            } label: {
                                Text(t("set.delete"))
                            }
                        }
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .searchable(text: $search, prompt: t("hist.search"))
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
    @State private var search = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var filtered: [TranslationHistoryEntry] {
        let q = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty { return store.userData.translationHistory }
        return store.userData.translationHistory.filter {
            $0.title.localizedCaseInsensitiveContains(q)
                || $0.url.localizedCaseInsensitiveContains(q)
                || $0.targetLanguage.localizedCaseInsensitiveContains(q)
                || $0.provider.localizedCaseInsensitiveContains(q)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                if filtered.isEmpty {
                    Text(t("trhist.empty")).foregroundStyle(.secondary)
                        .listRowBackground(Color.white.opacity(0.06))
                } else {
                    ForEach(filtered) { item in
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
                        .swipeActions {
                            Button(role: .destructive) {
                                store.deleteTranslationHistoryItem(item.id)
                            } label: {
                                Text(t("set.delete"))
                            }
                        }
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .searchable(text: $search, prompt: t("trhist.search"))
            .navigationTitle(t("trhist.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    if !store.userData.translationHistory.isEmpty {
                        Button(t("hist.clear"), role: .destructive) { store.clearTranslationHistory() }
                    }
                }
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

    @State private var selectedFolder = "__all__"
    @State private var targetFolder = "Default"
    @State private var searchQuery = ""
    @State private var editingId: String? = nil
    @State private var src = ""
    @State private var dst = ""
    @State private var caseSensitive = false
    @State private var exactMatch = false
    @State private var useRegex = false
    @State private var doNotTranslate = false
    @State private var noteDraft = ""

    @State private var showNewFolder = false
    @State private var showRenameFolder = false
    @State private var folderRenameTarget = "Default"
    @State private var folderDraft = ""
    @State private var showImportCSVAlert = false
    @State private var csvImportDraft = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var filteredGlossary: [GlossaryItem] {
        let q = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        return store.userData.glossary.filter { item in
            let matchesFolder = selectedFolder == "__all__" || item.folder == selectedFolder
            let matchesQuery = q.isEmpty
                || item.source.localizedCaseInsensitiveContains(q)
                || item.target.localizedCaseInsensitiveContains(q)
                || item.folder.localizedCaseInsensitiveContains(q)
            return matchesFolder && matchesQuery
        }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                ScrollView(.horizontal, showsIndicators: false) {
                    LiquidGlassGroup(spacing: 8) {
                        HStack(spacing: 8) {
                            folderChip(id: "__all__", title: t("gl.all"))
                            ForEach(store.userData.glossaryFolders, id: \.self) { folder in
                                folderChip(id: folder, title: folder)
                                    .contextMenu {
                                        if folder != "Default" {
                                            Button {
                                                folderRenameTarget = folder
                                                folderDraft = folder
                                                showRenameFolder = true
                                            } label: {
                                                Label(t("gl.renameFolder"), systemImage: "pencil")
                                            }
                                            Button(role: .destructive) {
                                                store.deleteGlossaryFolder(folder)
                                                if selectedFolder == folder { selectedFolder = "__all__" }
                                            } label: {
                                                Label(t("gl.deleteFolder"), systemImage: "trash")
                                            }
                                        }
                                    }
                            }
                            Button {
                                folderDraft = ""
                                showNewFolder = true
                            } label: {
                                Label(t("gl.newFolder"), systemImage: "folder.badge.plus")
                                    .font(.caption.bold())
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 7)
                                    .contentShape(Capsule())
                            }
                            .buttonStyle(LiquidPressButtonStyle())
                            .liquidGlassPill(interactive: true)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                }

                Form {
                    Section {
                        TextField(t("gl.source"), text: $src)
                        TextField(t("gl.target"), text: $dst)
                        Picker(t("gl.folder"), selection: $targetFolder) {
                            ForEach(store.userData.glossaryFolders, id: \.self) { folder in
                                Text(folder).tag(folder)
                            }
                        }
                        Toggle(t("gl.case"), isOn: $caseSensitive)
                        Toggle(t("gl.exact"), isOn: $exactMatch)
                        Toggle(t("gl.regex"), isOn: $useRegex)
                        Toggle(t("gl.dnt"), isOn: $doNotTranslate)
                        TextField(t("gl.note"), text: $noteDraft)

                        HStack {
                            Button(editingId == nil ? t("gl.add") : t("common.save")) {
                                store.upsertGlossaryItem(
                                    id: editingId,
                                    source: src,
                                    target: dst,
                                    caseSensitive: caseSensitive,
                                    folder: targetFolder,
                                    exactMatch: exactMatch,
                                    useRegex: useRegex,
                                    doNotTranslate: doNotTranslate,
                                    note: noteDraft
                                )
                                editingId = nil
                                src = ""
                                dst = ""
                                exactMatch = false
                                useRegex = false
                                doNotTranslate = false
                                noteDraft = ""
                            }
                            .buttonStyle(.borderless)
                            .disabled(src.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                            if editingId != nil {
                                Spacer()
                                Button(t("common.cancel"), role: .cancel) {
                                    editingId = nil
                                    src = ""
                                    dst = ""
                                    exactMatch = false
                                    useRegex = false
                                    doNotTranslate = false
                                    noteDraft = ""
                                }
                                .buttonStyle(.borderless)
                            }
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))

                    Section {
                        if filteredGlossary.isEmpty {
                            Text(t("gl.empty")).foregroundStyle(.secondary)
                        } else {
                            ForEach(filteredGlossary) { item in
                                HStack(spacing: 10) {
                                    Button {
                                        editingId = item.id
                                        src = item.source
                                        dst = item.target
                                        targetFolder = item.folder
                                        caseSensitive = item.caseSensitive
                                        exactMatch = item.exactMatch
                                        useRegex = item.useRegex
                                        doNotTranslate = item.doNotTranslate
                                        noteDraft = item.note
                                    } label: {
                                        VStack(alignment: .leading, spacing: 3) {
                                            HStack(spacing: 6) {
                                                Text(item.source).font(.subheadline.weight(.semibold))
                                                Text(item.folder)
                                                    .font(.caption2.bold())
                                                    .padding(.horizontal, 6)
                                                    .padding(.vertical, 2)
                                                    .background(FluoThemeTokens.indigo.opacity(0.18), in: Capsule())
                                                if item.exactMatch {
                                                    Text("Exact").font(.caption2).foregroundStyle(.secondary)
                                                }
                                                if item.useRegex {
                                                    Text("Regex").font(.caption2).foregroundStyle(.secondary)
                                                }
                                            }
                                            Text(item.doNotTranslate || item.target.isEmpty ? "(keep original)" : item.target)
                                                .font(.caption).foregroundStyle(.secondary)
                                            if !item.note.isEmpty {
                                                Text(item.note).font(.caption2).foregroundStyle(.secondary)
                                            }
                                        }
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                    .buttonStyle(.plain)

                                    Toggle("", isOn: Binding(
                                        get: { item.enabled },
                                        set: { _ in store.toggleGlossaryEnabled(item.id) }
                                    ))
                                    .labelsHidden()
                                    .tint(FluoThemeTokens.indigo)

                                    Button(role: .destructive) {
                                        store.deleteGlossaryItem(item.id)
                                    } label: {
                                        Image(systemName: "trash")
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                }
                .scrollContentBackground(.hidden)
                .searchable(text: $searchQuery, prompt: t("gl.search"))
            }
            .background { LiquidAuroraBackground() }
            .navigationTitle(t("gl.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Menu {
                        ShareLink(item: store.exportGlossaryCSV()) {
                            Label(t("gl.exportCsv"), systemImage: "square.and.arrow.up")
                        }
                        Button {
                            UIPasteboard.general.string = store.exportGlossaryCSV()
                        } label: {
                            Label(t("gl.exportCsv"), systemImage: "doc.on.doc")
                        }
                        Button {
                            csvImportDraft = UIPasteboard.general.string ?? ""
                            showImportCSVAlert = true
                        } label: {
                            Label(t("gl.importCsv"), systemImage: "square.and.arrow.down")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
            .alert(t("gl.importCsv"), isPresented: $showImportCSVAlert) {
                TextField("source,target,folder", text: $csvImportDraft)
                Button(t("common.cancel"), role: .cancel) {}
                Button(t("common.add")) {
                    let folder = selectedFolder == "__all__" ? "Default" : selectedFolder
                    _ = store.importGlossaryCSV(csvImportDraft, defaultFolder: folder)
                }
            }
            .alert(t("gl.newFolder"), isPresented: $showNewFolder) {
                TextField(t("gl.folder"), text: $folderDraft)
                Button(t("common.cancel"), role: .cancel) {}
                Button(t("common.add")) {
                    store.addGlossaryFolder(folderDraft)
                    let clean = folderDraft.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !clean.isEmpty {
                        selectedFolder = clean
                        targetFolder = clean
                    }
                }
            }
            .alert(t("gl.renameFolder"), isPresented: $showRenameFolder) {
                TextField(t("gl.folder"), text: $folderDraft)
                Button(t("common.cancel"), role: .cancel) {}
                Button(t("common.save")) {
                    let clean = folderDraft.trimmingCharacters(in: .whitespacesAndNewlines)
                    store.renameGlossaryFolder(old: folderRenameTarget, new: clean)
                    if !clean.isEmpty {
                        if selectedFolder == folderRenameTarget { selectedFolder = clean }
                        if targetFolder == folderRenameTarget { targetFolder = clean }
                    }
                }
            }
        }
    }

    private func folderChip(id: String, title: String) -> some View {
        let active = selectedFolder == id
        return Button {
            selectedFolder = id
            if id != "__all__" {
                targetFolder = id
            }
        } label: {
            HStack(spacing: 4) {
                Image(systemName: id == "__all__" ? "tray.full" : "folder.fill")
                    .font(.caption2)
                Text(title)
                    .font(.caption.weight(.bold))
            }
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
    @State private var confirmDeleteModelId: String? = nil

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

                    Toggle(t("tr.autoSetting"), isOn: $store.userData.autoTranslateEnabled)
                        .onChange(of: store.userData.autoTranslateEnabled) { _ in
                            store.savePreferences()
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
                                    HStack(spacing: 12) {
                                        Button {
                                            store.selectedLocalModel = spec.id
                                            store.savePreferences()
                                        } label: {
                                            Label(
                                                t("set.installed"),
                                                systemImage: store.selectedLocalModel == spec.id ? "checkmark.circle.fill" : "circle"
                                            )
                                            .frame(maxWidth: .infinity, alignment: .leading)
                                        }
                                        .buttonStyle(.borderless)

                                        Button(t("set.delete"), role: .destructive) {
                                            confirmDeleteModelId = spec.id
                                        }
                                        .buttonStyle(.borderless)
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
                            .buttonStyle(.borderless)
                            Spacer()
                            Button(t("set.clearKey"), role: .destructive) {
                                geminiKeyDraft = ""
                                KeychainStore.delete(account: "gemini_api_key")
                            }
                            .buttonStyle(.borderless)
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
                            .buttonStyle(.borderless)
                            Spacer()
                            Button(t("set.clearKey"), role: .destructive) {
                                openAiKeyDraft = ""
                                KeychainStore.delete(account: "openai_api_key")
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                    .listRowBackground(Color.white.opacity(0.08))
                }

                Section(t("set.backupSection")) {
                    if let json = store.userData.exportJSONString() {
                        ShareLink(item: json) {
                            Label(t("set.exportJson"), systemImage: "square.and.arrow.up")
                        }
                    }
                    Button {
                        if let json = store.userData.exportJSONString() {
                            UIPasteboard.general.string = json
                        }
                    } label: {
                        Label(t("set.exportJson"), systemImage: "doc.on.doc")
                    }
                    Button {
                        if let raw = UIPasteboard.general.string,
                           let imported = FluoUserDataSnapshot.importJSONString(raw) {
                            store.userData = imported
                            store.savePreferences()
                        }
                    } label: {
                        Label(t("set.importJson"), systemImage: "square.and.arrow.down")
                    }
                }
                .listRowBackground(Color.white.opacity(0.08))
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
            .alert(
                t("set.deleteConfirm"),
                isPresented: Binding(
                    get: { confirmDeleteModelId != nil },
                    set: { if !$0 { confirmDeleteModelId = nil } }
                )
            ) {
                Button(t("common.cancel"), role: .cancel) {
                    confirmDeleteModelId = nil
                }
                Button(t("set.delete"), role: .destructive) {
                    if let targetId = confirmDeleteModelId {
                        catalog.deleteModel(targetId)
                    }
                    confirmDeleteModelId = nil
                }
            }
        }
    }
}

struct DownloadsSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss
    @State private var search = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var filtered: [DownloadItemRecord] {
        let q = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty { return store.userData.downloads }
        return store.userData.downloads.filter {
            $0.fileName.localizedCaseInsensitiveContains(q) || $0.sourceURL.localizedCaseInsensitiveContains(q)
        }
    }

    private var totalBytes: Int64 {
        store.userData.downloads.reduce(0) { $0 + $1.byteSize }
    }

    var body: some View {
        NavigationStack {
            List {
                if filtered.isEmpty {
                    Text(t("dl.empty"))
                        .foregroundStyle(.secondary)
                        .listRowBackground(Color.white.opacity(0.06))
                } else {
                    Section {
                        ForEach(filtered) { item in
                            HStack(spacing: 12) {
                                Image(systemName: "doc.fill")
                                    .foregroundStyle(FluoThemeTokens.indigo)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(item.fileName)
                                        .font(.subheadline.weight(.semibold))
                                        .lineLimit(1)
                                    Text(ByteCountFormatter.string(fromByteCount: item.byteSize, countStyle: .file))
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                    Text(item.sourceURL)
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                Spacer()
                                if FileManager.default.fileExists(atPath: item.localPath) {
                                    ShareLink(item: URL(fileURLWithPath: item.localPath)) {
                                        Image(systemName: "square.and.arrow.up")
                                            .font(.subheadline)
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                            .listRowBackground(Color.white.opacity(0.08))
                            .swipeActions {
                                Button(role: .destructive) {
                                    try? FileManager.default.removeItem(atPath: item.localPath)
                                    store.userData.downloads.removeAll { $0.id == item.id }
                                    store.userData.save()
                                } label: {
                                    Text(t("set.delete"))
                                }
                            }
                        }
                    } footer: {
                        Text("\(store.userData.downloads.count) files · \(ByteCountFormatter.string(fromByteCount: totalBytes, countStyle: .file))")
                            .font(.caption2)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { LiquidAuroraBackground() }
            .searchable(text: $search, prompt: t("dl.search"))
            .navigationTitle(t("dl.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    if !store.userData.downloads.isEmpty {
                        Button(t("dl.clearAll"), role: .destructive) {
                            store.clearAllDownloads()
                        }
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}
