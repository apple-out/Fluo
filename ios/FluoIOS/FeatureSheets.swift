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
                    VStack(alignment: .leading, spacing: 24) {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Fluo")
                                .font(.system(size: 46, weight: .black, design: .serif))
                                .foregroundStyle(FluoThemeTokens.liquidGradient)
                            Text(t("app.tagline"))
                                .font(.caption.weight(.bold))
                                .padding(.horizontal, 12)
                                .padding(.vertical, 6)
                                .liquidGlassPill(tint: FluoThemeTokens.indigo)
                        }
                        .padding(.top, 8)

                        Text(t("onb.desc"))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        LiquidGlassGroup(spacing: 12) {
                            LazyVGrid(columns: [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)], spacing: 12) {
                                ForEach(AppLanguage.allCases) { lang in
                                    let selected = store.uiLang == lang
                                    Button {
                                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                        store.uiLang = lang
                                        store.targetLanguage = lang.defaultTargetLanguage
                                        store.savePreferences()
                                    } label: {
                                        VStack(alignment: .leading, spacing: 10) {
                                            HStack {
                                                Text(lang.shortCode)
                                                    .font(.caption.weight(.black))
                                                    .padding(.horizontal, 8)
                                                    .padding(.vertical, 4)
                                                    .background(selected ? Color.white.opacity(0.24) : FluoThemeTokens.indigo.opacity(0.16), in: Capsule())
                                                    .foregroundStyle(selected ? .white : FluoThemeTokens.indigo)
                                                Spacer()
                                                if selected {
                                                    Image(systemName: "checkmark.circle.fill")
                                                        .foregroundStyle(.white)
                                                }
                                            }
                                            Text(lang.nativeName)
                                                .font(.headline.weight(.bold))
                                                .foregroundStyle(selected ? .white : .primary)
                                        }
                                        .padding(16)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())
                                    .liquidGlass(
                                        cornerRadius: 22,
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
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)
                                .textCase(.uppercase)
                            TextField(t("onb.targetLabel"), text: $store.targetLanguage)
                                .font(.body.weight(.medium))
                                .padding(.horizontal, 16)
                                .padding(.vertical, 14)
                                .liquidGlass(cornerRadius: 20)
                        }

                        Button {
                            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                            store.onboardingCompleted = true
                            store.savePreferences()
                            dismiss()
                        } label: {
                            HStack(spacing: 8) {
                                Text(t("onb.startBtn"))
                                    .font(.headline.weight(.bold))
                                Image(systemName: "arrow.right")
                                    .font(.subheadline.weight(.bold))
                            }
                            .foregroundStyle(.white)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 16)
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
    var onClose: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private func closeSheet() {
        onClose?()
        dismiss()
    }

    private func dismissAndRun(_ work: @escaping () -> Void) {
        closeSheet()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
            work()
        }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                LiquidAuroraBackground()

                ScrollView {
                    VStack(spacing: 16) {
                        if store.provider == .hyMt2 && !catalog.isInstalled(store.selectedLocalModel) {
                            VStack(alignment: .leading, spacing: 12) {
                                HStack(spacing: 10) {
                                    SpatialIconMedallion(icon: "arrow.down.circle.fill", tint: FluoThemeTokens.indigo, size: 36)
                                    Text(t("tr.needModel"))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
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
                                            .padding(.vertical, 12)
                                            .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: false, prominent: true)
                                            .contentShape(Capsule())
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())
                                }
                            }
                            .padding(16)
                            .liquidGlass(cornerRadius: 24, tint: FluoThemeTokens.indigo)
                        }

                        if let err = store.translationError {
                            Text(err)
                                .font(.footnote)
                                .foregroundStyle(.red)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(14)
                                .liquidGlass(cornerRadius: 18, tint: .red)
                        }

                        // 3-Column Display Mode Hero Cards
                        HStack(spacing: 10) {
                            displayModeCard(
                                title: t("tr.original"),
                                icon: "doc.plaintext",
                                tint: FluoThemeTokens.cyan,
                                active: store.activeTab.displayMode == .original
                            ) {
                                dismissAndRun { store.restoreOriginal() }
                            }

                            displayModeCard(
                                title: t("tr.inline"),
                                icon: "sparkles",
                                tint: FluoThemeTokens.indigo,
                                active: store.activeTab.displayMode == .inline
                            ) {
                                dismissAndRun { store.translateActiveTab(bilingual: false) }
                            }

                            displayModeCard(
                                title: t("tr.bilingual"),
                                icon: "rectangle.split.2x1",
                                tint: FluoThemeTokens.violet,
                                active: store.activeTab.displayMode == .bilingual
                            ) {
                                dismissAndRun { store.translateActiveTab(bilingual: true) }
                            }
                        }

                        // 4-Column Tactile Reading Actions Strip
                        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4), spacing: 8) {
                            studioToolCard(
                                title: t("tr.regionPicker"),
                                icon: "hand.tap",
                                tint: FluoThemeTokens.amber,
                                active: store.isPickingRegion
                            ) {
                                dismissAndRun { store.startRegionPicker(bilingual: false) }
                            }

                            studioToolCard(
                                title: t("tr.selection"),
                                icon: "text.quote",
                                tint: FluoThemeTokens.emerald,
                                active: false
                            ) {
                                dismissAndRun { store.translateSelectedText() }
                            }

                            studioToolCard(
                                title: t("tr.aiSummary"),
                                icon: "sparkles.rectangle.stack.fill",
                                tint: FluoThemeTokens.violet,
                                active: false
                            ) {
                                dismissAndRun { store.summarizeActivePage() }
                            }

                            studioToolCard(
                                title: t("tr.tts"),
                                icon: store.isSpeakingTTS ? "speaker.wave.3.fill" : "speaker.wave.2.fill",
                                tint: FluoThemeTokens.cyan,
                                active: store.isSpeakingTTS
                            ) {
                                dismissAndRun { store.startOrToggleTTS() }
                            }
                        }

                        if !store.currentHost.isEmpty {
                            HStack(spacing: 10) {
                                SpatialIconMedallion(icon: "globe.badge.chevron.backward", tint: FluoThemeTokens.indigo, size: 32)
                                Text(t("tr.alwaysHost"))
                                    .font(.subheadline.weight(.semibold))
                                    .lineLimit(1)
                                Spacer()
                                Toggle("", isOn: Binding(
                                    get: { store.isCurrentHostAlwaysTranslated },
                                    set: { _ in store.toggleAlwaysTranslateCurrentHost() }
                                ))
                                .labelsHidden()
                                .tint(FluoThemeTokens.indigo)
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 10)
                            .liquidGlass(cornerRadius: 20)
                        }

                        // Style & Typography Studio Card
                        VStack(alignment: .leading, spacing: 14) {
                            Text(t("tr.style"))
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)
                                .textCase(.uppercase)

                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 8) {
                                    ForEach(TranslationStyle.allCases) { st in
                                        let active = store.userData.style == st
                                        Button {
                                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                            store.userData.style = st
                                            store.savePreferences()
                                        } label: {
                                            Text(t("style.\(st.rawValue)"))
                                                .font(.caption.weight(.bold))
                                                .padding(.horizontal, 13)
                                                .padding(.vertical, 8)
                                                .foregroundStyle(active ? .white : .primary)
                                                .background {
                                                    if active {
                                                        Capsule().fill(FluoThemeTokens.liquidGradient)
                                                    } else {
                                                        Capsule().fill(Color.primary.opacity(0.08))
                                                    }
                                                }
                                                .contentShape(Capsule())
                                        }
                                        .buttonStyle(LiquidPressButtonStyle())
                                    }
                                }
                                .padding(.vertical, 2)
                            }

                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text(t("tr.fontScale")).font(.caption.weight(.semibold))
                                    Spacer()
                                    Text(String(format: "%.2fx", store.userData.fontScale))
                                        .font(.caption.monospacedDigit().weight(.bold))
                                }
                                Slider(value: $store.userData.fontScale, in: 0.8...1.6, step: 0.05)
                                    .tint(FluoThemeTokens.indigo)
                                    .onChange(of: store.userData.fontScale) { _ in store.savePreferences() }
                            }

                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text(t("tr.lineHeight")).font(.caption.weight(.semibold))
                                    Spacer()
                                    Text(String(format: "%.1f", store.userData.lineHeight))
                                        .font(.caption.monospacedDigit().weight(.bold))
                                }
                                Slider(value: $store.userData.lineHeight, in: 1.2...2.2, step: 0.1)
                                    .tint(FluoThemeTokens.indigo)
                                    .onChange(of: store.userData.lineHeight) { _ in store.savePreferences() }
                            }

                            Divider().opacity(0.25)

                            Button {
                                openSettings()
                            } label: {
                                HStack(spacing: 10) {
                                    SpatialIconMedallion(icon: "cpu", tint: FluoThemeTokens.indigo, size: 30)
                                    Text("\(store.provider.rawValue) · \(store.targetLanguage)")
                                        .font(.subheadline.weight(.semibold))
                                    Spacer()
                                    Image(systemName: "chevron.right")
                                        .font(.caption.bold())
                                        .foregroundStyle(.secondary)
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(16)
                        .liquidGlass(cornerRadius: 24)
                    }
                    .padding(16)
                }
            }
            .navigationTitle(t("sheet.translateTitle"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) {
                        closeSheet()
                    }
                }
            }
        }
    }

    private func displayModeCard(title: String, icon: String, tint: Color, active: Bool, action: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            action()
        } label: {
            VStack(spacing: 10) {
                SpatialIconMedallion(icon: icon, tint: tint, size: 36, active: active)
                Text(title)
                    .font(.caption.weight(.bold))
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(active ? .white : .primary)
            }
            .frame(maxWidth: .infinity, minHeight: 96)
            .padding(10)
            .liquidGlass(
                cornerRadius: 22,
                tint: active ? tint : .clear,
                interactive: false,
                prominent: active
            )
            .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
    }

    private func studioToolCard(title: String, icon: String, tint: Color, active: Bool, action: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            action()
        } label: {
            VStack(spacing: 7) {
                SpatialIconMedallion(icon: icon, tint: tint, size: 32, active: active)
                Text(title)
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(active ? .white : .primary)
            }
            .frame(maxWidth: .infinity, minHeight: 78)
            .padding(.horizontal, 6)
            .padding(.vertical, 8)
            .liquidGlass(
                cornerRadius: 20,
                tint: active ? tint : .clear,
                interactive: false,
                prominent: active
            )
            .contentShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
    }
}

struct TabsSheet: View {
    @ObservedObject var store: BrowserStore
    var openWorkspaces: (() -> Void)? = nil
    var onClose: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss

    @State private var tabSearch = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private func closeSheet() {
        onClose?()
        dismiss()
    }

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

    private func cleanHost(for urlString: String) -> String {
        guard let host = URL(string: urlString)?.host, !host.isEmpty else { return "" }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 10) {
                // Inline Glass Tab Search Bar
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.secondary)
                    TextField(t("tabs.search"), text: $tabSearch)
                        .font(.subheadline)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    if !tabSearch.isEmpty {
                        Button {
                            tabSearch = ""
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .liquidGlassPill()
                .padding(.horizontal, 16)
                .padding(.top, 8)

                // Gesture hint & active tab count row
                HStack(spacing: 6) {
                    Image(systemName: "hand.draw")
                        .font(.caption2.weight(.semibold))
                    Text(t("tabs.swipeHint"))
                        .font(.caption2.weight(.medium))
                    Spacer(minLength: 8)
                    Text("\(filteredTabs.count) \(t("tabs.countFmt"))")
                        .font(.caption2.monospacedDigit().weight(.bold))
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 20)

                // Gesture-driven Vertical Tab Deck (swipe left/right to close)
                ScrollView {
                    LazyVStack(spacing: 12) {
                        ForEach(filteredTabs) { tab in
                            let isCurrent = tab.id == store.activeTabId
                            let host = tab.isStartPage ? t("start.title") : cleanHost(for: tab.urlString)
                            let closable = !tab.isPinned && (store.activeWorkspaceTabs.count > 1 || !tab.isStartPage)

                            SwipeableTabCard(
                                tab: tab,
                                isCurrent: isCurrent,
                                host: host,
                                closable: closable,
                                workspaces: store.userData.workspaces,
                                t: t,
                                onSelect: {
                                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                    store.selectTab(tab.id)
                                    closeSheet()
                                },
                                onClose: {
                                    store.closeTab(tab.id)
                                },
                                onTogglePin: {
                                    store.togglePinTab(tab.id)
                                },
                                onDuplicate: {
                                    store.duplicateTab(tab.id)
                                },
                                onSleep: {
                                    tab.putToSleep()
                                },
                                onMoveWorkspace: { wsId in
                                    store.moveTab(tab.id, toWorkspace: wsId)
                                }
                            )
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
                    .padding(.bottom, 16)
                }
            }
            .background { LiquidAuroraBackground().ignoresSafeArea() }
            .safeAreaInset(edge: .bottom) {
                LiquidGlassGroup(spacing: 10) {
                    HStack(spacing: 10) {
                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            store.newTab(isPrivate: true)
                            closeSheet()
                        } label: {
                            Image(systemName: "eye.slash.fill")
                                .font(.system(size: 15, weight: .bold))
                                .foregroundStyle(FluoThemeTokens.violet)
                                .frame(width: 44, height: 44)
                                .liquidGlassCircle(tint: FluoThemeTokens.violet, interactive: false)
                                .contentShape(Circle())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .accessibilityLabel(t("tabs.newPrivate"))

                        Button {
                            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                            store.newTab(isPrivate: false)
                            closeSheet()
                        } label: {
                            HStack(spacing: 7) {
                                Image(systemName: "plus")
                                    .font(.subheadline.weight(.bold))
                                Text(t("tabs.newTab"))
                                    .font(.subheadline.weight(.bold))
                            }
                            .foregroundStyle(.white)
                            .frame(maxWidth: .infinity)
                            .frame(height: 44)
                            .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: false, prominent: true)
                            .contentShape(Capsule())
                        }
                        .buttonStyle(LiquidPressButtonStyle())

                        if !store.closedTabs.isEmpty {
                            Button {
                                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                store.reopenClosedTab()
                                closeSheet()
                            } label: {
                                Image(systemName: "arrow.uturn.left")
                                    .font(.system(size: 15, weight: .bold))
                                    .frame(width: 44, height: 44)
                                    .liquidGlassCircle(interactive: false)
                                    .contentShape(Circle())
                            }
                            .buttonStyle(LiquidPressButtonStyle())
                            .accessibilityLabel(t("tabs.reopen"))
                        }

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
                            Image(systemName: "ellipsis")
                                .font(.system(size: 15, weight: .bold))
                                .frame(width: 44, height: 44)
                                .liquidGlassCircle(interactive: false)
                                .contentShape(Circle())
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                }
            }
            .navigationTitle(activeWorkspace?.name ?? t("tabs.onlyTitle"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if let openWorkspaces {
                    ToolbarItem(placement: .navigationBarLeading) {
                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            openWorkspaces()
                        } label: {
                            HStack(spacing: 5) {
                                Image(systemName: "square.stack.3d.up.fill")
                                Text(t("ws.title"))
                            }
                            .font(.subheadline.weight(.semibold))
                        }
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) { closeSheet() }
                }
            }
        }
    }
}

private struct SwipeableTabCard: View {
    @ObservedObject var tab: BrowserTab
    let isCurrent: Bool
    let host: String
    let closable: Bool
    let workspaces: [WorkspaceItem]
    let t: (String) -> String
    let onSelect: () -> Void
    let onClose: () -> Void
    let onTogglePin: () -> Void
    let onDuplicate: () -> Void
    let onSleep: () -> Void
    let onMoveWorkspace: (String) -> Void

    @GestureState private var liveDragX: CGFloat = 0
    @State private var dismissedOffset: CGFloat = 0
    @State private var crossedThreshold = false
    @State private var isClosing = false

    private var activeOffset: CGFloat {
        isClosing ? dismissedOffset : liveDragX
    }

    var body: some View {
        ZStack {
            if closable && abs(activeOffset) > 6 {
                let thresholdReached = abs(activeOffset) >= 92
                RoundedRectangle(cornerRadius: 22, style: .continuous)
                    .fill(Color.red.opacity(min(0.88, 0.22 + abs(activeOffset) / 140.0)))
                    .overlay {
                        HStack {
                            if activeOffset > 0 {
                                Image(systemName: "trash.fill")
                                    .font(.title3.weight(.bold))
                                    .foregroundStyle(.white)
                                    .scaleEffect(thresholdReached ? 1.15 : 0.92)
                                    .padding(.leading, 20)
                                Spacer()
                            } else {
                                Spacer()
                                Image(systemName: "trash.fill")
                                    .font(.title3.weight(.bold))
                                    .foregroundStyle(.white)
                                    .scaleEffect(thresholdReached ? 1.15 : 0.92)
                                    .padding(.trailing, 20)
                            }
                        }
                    }
            }

            HStack(spacing: 12) {
                Button {
                    guard !isClosing else { return }
                    onSelect()
                } label: {
                    HStack(spacing: 12) {
                        DomainMonogramBadge(
                            title: tab.title,
                            host: tab.isStartPage ? "" : host,
                            size: 40,
                            cornerRadius: 12,
                            symbolFallback: tab.isStartPage ? "sparkles" : "book.closed.fill"
                        )

                        VStack(alignment: .leading, spacing: 4) {
                            Text(tab.title.isEmpty ? (host.isEmpty ? "Fluo" : host) : tab.title)
                                .font(.subheadline.weight(isCurrent ? .bold : .semibold))
                                .foregroundStyle(.primary)
                                .lineLimit(2)
                                .multilineTextAlignment(.leading)

                            HStack(spacing: 6) {
                                Text(host.isEmpty ? "Fluo" : host)
                                    .font(.caption2.weight(.medium))
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)

                                if tab.isPinned {
                                    Image(systemName: "pin.fill")
                                        .font(.system(size: 10, weight: .bold))
                                        .foregroundStyle(FluoThemeTokens.indigo)
                                }
                                if tab.isPrivate {
                                    Text(t("tabs.privateBadge"))
                                        .font(.system(size: 10, weight: .bold))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(FluoThemeTokens.violet.opacity(0.24), in: Capsule())
                                }
                                if tab.isSleeping {
                                    Text(t("tabs.sleepingBadge"))
                                        .font(.system(size: 10, weight: .bold))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(Color.secondary.opacity(0.22), in: Capsule())
                                }
                                if tab.displayMode != .original {
                                    Text(tab.displayMode == .bilingual ? "BI" : "TR")
                                        .font(.system(size: 10, weight: .black))
                                        .foregroundStyle(FluoThemeTokens.indigo)
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(FluoThemeTokens.indigo.opacity(0.18), in: Capsule())
                                }
                            }
                        }

                        Spacer(minLength: 4)
                    }
                    .padding(.vertical, 12)
                    .padding(.leading, 14)
                    .padding(.trailing, closable ? 4 : 14)
                    .frame(maxWidth: .infinity, minHeight: 74, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                if closable {
                    Button {
                        guard !isClosing else { return }
                        isClosing = true
                        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                        withAnimation(.easeOut(duration: 0.14)) {
                            dismissedOffset = -28
                        }
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
                            onClose()
                        }
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .font(.system(size: 21))
                            .foregroundStyle(.secondary)
                            .frame(width: 40, height: 40)
                            .contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .padding(.trailing, 8)
                }
            }
            .liquidGlass(
                cornerRadius: 22,
                tint: tab.isPrivate
                    ? FluoThemeTokens.violet
                    : (isCurrent ? FluoThemeTokens.indigo : .clear),
                interactive: false
            )
            .overlay {
                if isCurrent {
                    RoundedRectangle(cornerRadius: 22, style: .continuous)
                        .strokeBorder(FluoThemeTokens.liquidGradient, lineWidth: 1.8)
                }
            }
            .offset(x: activeOffset)
            .rotationEffect(.degrees(Double(activeOffset) / 42.0))
            .opacity(max(0.22, 1.0 - Double(abs(activeOffset)) / 280.0))
            .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: 22, style: .continuous))
            .contextMenu {
                Button {
                    onTogglePin()
                } label: {
                    Label(tab.isPinned ? t("tabs.unpin") : t("tabs.pin"), systemImage: "pin")
                }

                Button {
                    onDuplicate()
                } label: {
                    Label(t("tabs.duplicate"), systemImage: "plus.square.on.square")
                }

                if !tab.isStartPage && !isCurrent && !tab.isSleeping {
                    Button {
                        onSleep()
                    } label: {
                        Label(t("tabs.sleep"), systemImage: "moon.zzz")
                    }
                }

                Menu(t("tabs.moveWs")) {
                    ForEach(workspaces) { ws in
                        Button(ws.name) { onMoveWorkspace(ws.id) }
                    }
                }

                if closable {
                    Button(role: .destructive) {
                        onClose()
                    } label: {
                        Label(t("set.delete"), systemImage: "xmark.circle")
                    }
                }
            }
            .simultaneousGesture(
                DragGesture(minimumDistance: 18, coordinateSpace: .local)
                    .updating($liveDragX) { value, state, _ in
                        guard closable, !isClosing else { return }
                        let dx = value.translation.width
                        let dy = value.translation.height
                        if abs(dx) > abs(dy) * 1.2 && abs(dx) > 14 {
                            state = dx
                        } else {
                            state = 0
                        }
                    }
                    .onChanged { value in
                        guard closable, !isClosing else { return }
                        let dx = value.translation.width
                        let dy = value.translation.height
                        let pastThreshold = abs(dx) >= 92 && abs(dx) > abs(dy) * 1.2
                        if pastThreshold && !crossedThreshold {
                            crossedThreshold = true
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        } else if !pastThreshold {
                            crossedThreshold = false
                        }
                    }
                    .onEnded { value in
                        crossedThreshold = false
                        guard closable, !isClosing else { return }
                        let dx = value.translation.width
                        let dy = value.translation.height
                        let horizontal = abs(dx) > abs(dy) * 1.15
                        let predicted = value.predictedEndTranslation.width
                        if horizontal && (abs(dx) >= 92 || abs(predicted) >= 170) {
                            isClosing = true
                            dismissedOffset = dx
                            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                            withAnimation(.easeOut(duration: 0.16)) {
                                dismissedOffset = (dx >= 0 ? 420 : -420)
                            }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                                onClose()
                            }
                        }
                    }
            )
        }
    }
}

struct WorkspacesSheet: View {
    @ObservedObject var store: BrowserStore
    var onClose: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss

    @State private var showNewWorkspace = false
    @State private var showRenameWorkspace = false
    @State private var wsTargetId = ""
    @State private var wsDraftName = ""
    @State private var pendingDeleteWorkspace: WorkspaceItem?

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private func closeSheet() {
        onClose?()
        dismiss()
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 12) {
                    ForEach(store.userData.workspaces) { ws in
                        let isActive = store.userData.activeWorkspaceId == ws.id
                        let tabCount = store.tabs.filter { $0.workspaceId == ws.id }.count

                        HStack(spacing: 12) {
                            Button {
                                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                                store.switchWorkspace(ws.id)
                                closeSheet()
                            } label: {
                                HStack(spacing: 12) {
                                    SpatialIconMedallion(
                                        icon: isActive ? "checkmark.circle.fill" : "square.stack.3d.up.fill",
                                        tint: isActive ? FluoThemeTokens.indigo : FluoThemeTokens.violet,
                                        size: 42,
                                        active: isActive
                                    )

                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(ws.name)
                                            .font(.headline.weight(.bold))
                                            .foregroundStyle(.primary)
                                            .lineLimit(1)
                                        Text("\(tabCount) \(t("tabs.countFmt"))")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer(minLength: 4)
                                }
                                .padding(.vertical, 14)
                                .padding(.leading, 14)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)

                            HStack(spacing: 6) {
                                Button {
                                    wsTargetId = ws.id
                                    wsDraftName = ws.name
                                    showRenameWorkspace = true
                                } label: {
                                    Image(systemName: "pencil")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(.secondary)
                                        .frame(width: 36, height: 36)
                                        .contentShape(Circle())
                                }
                                .buttonStyle(.plain)
                                .accessibilityLabel(t("ws.rename"))

                                if ws.id != "default" {
                                    Button {
                                        pendingDeleteWorkspace = ws
                                    } label: {
                                        Image(systemName: "trash")
                                            .font(.subheadline.weight(.semibold))
                                            .foregroundStyle(.red)
                                            .frame(width: 36, height: 36)
                                            .contentShape(Circle())
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityLabel(t("ws.delete"))
                                }
                            }
                            .padding(.trailing, 10)
                        }
                        .liquidGlass(
                            cornerRadius: 22,
                            tint: isActive ? FluoThemeTokens.indigo : .clear,
                            interactive: false
                        )
                        .overlay {
                            if isActive {
                                RoundedRectangle(cornerRadius: 22, style: .continuous)
                                    .strokeBorder(FluoThemeTokens.liquidGradient, lineWidth: 1.6)
                            }
                        }
                    }
                }
                .padding(16)
            }
            .background { LiquidAuroraBackground().ignoresSafeArea() }
            .safeAreaInset(edge: .bottom) {
                Button {
                    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                    wsDraftName = ""
                    showNewWorkspace = true
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "plus")
                            .font(.subheadline.weight(.bold))
                        Text(t("ws.new"))
                            .font(.subheadline.weight(.bold))
                    }
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .frame(height: 46)
                    .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: false, prominent: true)
                    .contentShape(Capsule())
                }
                .buttonStyle(LiquidPressButtonStyle())
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
            }
            .navigationTitle(t("ws.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) { closeSheet() }
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
            .alert(
                t("ws.delete"),
                isPresented: Binding(
                    get: { pendingDeleteWorkspace != nil },
                    set: { if !$0 { pendingDeleteWorkspace = nil } }
                )
            ) {
                Button(t("common.cancel"), role: .cancel) {
                    pendingDeleteWorkspace = nil
                }
                Button(t("ws.delete"), role: .destructive) {
                    if let ws = pendingDeleteWorkspace {
                        store.deleteWorkspace(ws.id)
                    }
                    pendingDeleteWorkspace = nil
                }
            } message: {
                Text(t("ws.deleteConfirm"))
            }
        }
    }
}

struct MenuSheet: View {
    @ObservedObject var store: BrowserStore
    var openSheet: (ActiveSheetKind) -> Void
    var onClose: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss

    @State private var copiedFeedback = false
    @State private var readingListAdded = false
    @State private var isExpanded = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private func closeSheet() {
        onClose?()
        dismiss()
    }

    private func dismissAndRun(_ action: @escaping () -> Void) {
        closeSheet()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
            action()
        }
    }

    private var activeWorkspaceName: String {
        store.userData.workspaces.first(where: { $0.id == store.userData.activeWorkspaceId })?.name ?? ""
    }

    var body: some View {
        NavigationStack {
            ScrollView {
            VStack(spacing: 16) {
                // Primary 2x2 Bento Grid (immediately visible)
                LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                    heroBentoTile(
                        title: t("menu.readerMode"),
                        icon: "doc.text.magnifyingglass",
                        tint: FluoThemeTokens.indigo,
                        active: false,
                        disabled: store.activeTab.isStartPage
                    ) {
                        dismissAndRun {
                            store.toggleReaderMode()
                        }
                    }

                    heroBentoTile(
                        title: t("menu.bookmarks"),
                        icon: "books.vertical.fill",
                        tint: FluoThemeTokens.amber,
                        active: false
                    ) {
                        openSheet(.bookmarks)
                    }

                    heroBentoTile(
                        title: t("bar.translate"),
                        icon: "sparkles",
                        tint: FluoThemeTokens.violet,
                        active: store.activeTab.displayMode != .original
                    ) {
                        openSheet(.translate)
                    }

                    if !store.activeTab.isStartPage,
                       let shareUrl = URL(string: BrowserStore.cleanURLWithoutTrackers(store.activeTab.urlString)) {
                        ShareLink(item: shareUrl) {
                            heroBentoLabel(
                                title: t("menu.share"),
                                icon: "square.and.arrow.up",
                                tint: FluoThemeTokens.cyan,
                                active: false
                            )
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                    } else {
                        heroBentoTile(
                            title: t("menu.share"),
                            icon: "square.and.arrow.up",
                            tint: FluoThemeTokens.cyan,
                            active: false,
                            disabled: true
                        ) {}
                    }
                }

                // Expand / Collapse Toggle Pill
                Button {
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    withAnimation(.spring(response: 0.28, dampingFraction: 0.85)) {
                        isExpanded.toggle()
                    }
                } label: {
                    HStack(spacing: 8) {
                        Text(isExpanded ? t("menu.less") : t("menu.more"))
                            .font(.subheadline.weight(.bold))
                        Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                            .font(.caption.weight(.bold))
                    }
                    .foregroundStyle(.primary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                    .liquidGlassPill(interactive: false)
                    .contentShape(Capsule())
                }
                .buttonStyle(LiquidPressButtonStyle())

                if isExpanded {
                    VStack(spacing: 16) {
                        // Dedicated Large Workspaces Card
                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            openSheet(.workspaces)
                        } label: {
                            HStack(spacing: 12) {
                                SpatialIconMedallion(
                                    icon: "square.stack.3d.up.fill",
                                    tint: FluoThemeTokens.violet,
                                    size: 38
                                )
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(t("ws.title"))
                                        .font(.subheadline.weight(.bold))
                                        .foregroundStyle(.primary)
                                    Text("\(activeWorkspaceName) · \(store.activeWorkspaceTabs.count) \(t("tabs.countFmt"))")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer(minLength: 0)
                                Image(systemName: "chevron.right")
                                    .font(.caption.weight(.bold))
                                    .foregroundStyle(.secondary)
                            }
                            .padding(14)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .liquidGlass(cornerRadius: 22, interactive: false)
                            .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                        }
                        .buttonStyle(LiquidPressButtonStyle())

                        if !store.activeTab.isStartPage {
                            // Current Page Identity Header Pill
                            HStack(spacing: 12) {
                                DomainMonogramBadge(
                                    title: store.activeTab.title,
                                    host: store.currentHost,
                                    size: 38,
                                    cornerRadius: 12
                                )
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(store.activeTab.title.isEmpty ? store.currentHost : store.activeTab.title)
                                        .font(.subheadline.weight(.bold))
                                        .lineLimit(1)
                                    Text(store.currentHost)
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                Spacer()
                                Button {
                                    openSheet(.bookmarkEditor)
                                } label: {
                                    Image(systemName: store.isCurrentPageBookmarked ? "star.fill" : "star")
                                        .font(.system(size: 16, weight: .bold))
                                        .foregroundStyle(store.isCurrentPageBookmarked ? .white : FluoThemeTokens.indigo)
                                        .frame(width: 36, height: 36)
                                        .liquidGlassCircle(
                                            tint: FluoThemeTokens.indigo,
                                            interactive: false,
                                            prominent: store.isCurrentPageBookmarked
                                        )
                                        .contentShape(Circle())
                                }
                                .buttonStyle(LiquidPressButtonStyle())
                            }
                            .padding(12)
                            .liquidGlass(cornerRadius: 22)

                            // Secondary Reading Bento Row (Smart Dark, TTS, Reading List)
                            LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                                heroBentoTile(
                                    title: t("menu.smartDark"),
                                    icon: "moon.stars.fill",
                                    tint: FluoThemeTokens.violet,
                                    active: false
                                ) {
                                    dismissAndRun {
                                        store.toggleSmartDarkPage()
                                    }
                                }

                                heroBentoTile(
                                    title: t("tr.tts"),
                                    icon: store.isSpeakingTTS ? "speaker.wave.3.fill" : "speaker.wave.2.fill",
                                    tint: FluoThemeTokens.cyan,
                                    active: store.isSpeakingTTS
                                ) {
                                    dismissAndRun {
                                        store.startOrToggleTTS()
                                    }
                                }

                                heroBentoTile(
                                    title: t("menu.readingListAdd"),
                                    icon: "eyeglasses",
                                    tint: FluoThemeTokens.amber,
                                    active: readingListAdded
                                ) {
                                    store.addCurrentPageToReadingList()
                                    readingListAdded = true
                                }
                            }

                            // Compact Page Controls Strip (Zoom + Find + Desktop + AdBlock)
                            VStack(spacing: 12) {
                                HStack(spacing: 10) {
                                    Label(t("set.zoom"), systemImage: "textformat.size")
                                        .font(.caption.weight(.bold))
                                        .foregroundStyle(.secondary)
                                    Spacer()
                                    Button {
                                        store.activeTab.applyPageZoom(store.activeTab.pageZoom - 0.1)
                                    } label: {
                                        Image(systemName: "minus.circle.fill")
                                            .font(.title3)
                                            .foregroundStyle(.secondary)
                                    }
                                    .buttonStyle(.plain)

                                    Text("\(Int((store.activeTab.pageZoom * 100).rounded()))%")
                                        .font(.caption.monospacedDigit().weight(.black))
                                        .frame(minWidth: 46)
                                        .onTapGesture {
                                            store.activeTab.applyPageZoom(1.0)
                                        }

                                    Button {
                                        store.activeTab.applyPageZoom(store.activeTab.pageZoom + 0.1)
                                    } label: {
                                        Image(systemName: "plus.circle.fill")
                                            .font(.title3)
                                            .foregroundStyle(FluoThemeTokens.indigo)
                                    }
                                    .buttonStyle(.plain)
                                }

                                Divider().opacity(0.25)

                                HStack(spacing: 8) {
                                    compactTogglePill(
                                        title: t("menu.find"),
                                        icon: "magnifyingglass",
                                        active: store.showFindBar
                                    ) {
                                        dismissAndRun {
                                            store.showFindBar = true
                                        }
                                    }

                                    compactTogglePill(
                                        title: t("menu.desktop"),
                                        icon: "desktopcomputer",
                                        active: store.activeTab.isDesktopMode
                                    ) {
                                        let next = !store.activeTab.isDesktopMode
                                        dismissAndRun {
                                            store.activeTab.applyDesktopMode(next)
                                        }
                                    }

                                    compactTogglePill(
                                        title: t("menu.adblockSite"),
                                        icon: "shield.lefthalf.filled",
                                        active: store.isCurrentHostAdBlockEnabled
                                    ) {
                                        if !store.currentHost.isEmpty {
                                            store.toggleCurrentHostAdBlock()
                                        }
                                        closeSheet()
                                    }
                                }
                            }
                            .padding(14)
                            .liquidGlass(cornerRadius: 22)

                            // Contextual Copy Clean URL, PDF & Export Strip
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 8) {
                                    Button {
                                        UIPasteboard.general.string = BrowserStore.cleanURLWithoutTrackers(store.activeTab.urlString)
                                        copiedFeedback = true
                                    } label: {
                                        Label(copiedFeedback ? t("menu.copied") : t("menu.cleanUrl"), systemImage: "doc.on.doc")
                                            .font(.caption.weight(.bold))
                                            .padding(.horizontal, 14)
                                            .padding(.vertical, 10)
                                            .liquidGlassPill(tint: copiedFeedback ? FluoThemeTokens.emerald : .clear, interactive: false)
                                            .contentShape(Capsule())
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())

                                    Button {
                                        dismissAndRun {
                                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
                                                store.printCurrentPage()
                                            }
                                        }
                                    } label: {
                                        Label(t("menu.printPdf"), systemImage: "printer")
                                            .font(.caption.weight(.bold))
                                            .padding(.horizontal, 14)
                                            .padding(.vertical, 10)
                                            .liquidGlassPill(interactive: false)
                                            .contentShape(Capsule())
                                    }
                                    .buttonStyle(LiquidPressButtonStyle())

                                    if store.activeTab.displayMode != .original {
                                        Button {
                                            Task {
                                                let md = await store.exportCurrentPageMarkdown()
                                                UIPasteboard.general.string = md
                                                copiedFeedback = true
                                            }
                                        } label: {
                                            Label(t("menu.exportMd"), systemImage: "doc.richtext")
                                                .font(.caption.weight(.bold))
                                                .padding(.horizontal, 14)
                                                .padding(.vertical, 10)
                                                .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: false)
                                                .contentShape(Capsule())
                                        }
                                        .buttonStyle(LiquidPressButtonStyle())
                                    }
                                }
                                .padding(.vertical, 2)
                            }
                        }

                        // Library & Tools 2-Column Bento Grid
                        LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                            libraryBentoTile(title: t("menu.history"), icon: "clock.fill", tint: FluoThemeTokens.cyan) {
                                openSheet(.history)
                            }
                            libraryBentoTile(title: t("menu.trHistory"), icon: "clock.arrow.2.circlepath", tint: FluoThemeTokens.violet) {
                                openSheet(.translationHistory)
                            }
                            libraryBentoTile(title: t("menu.glossary"), icon: "character.book.closed.fill", tint: FluoThemeTokens.emerald) {
                                openSheet(.glossary)
                            }
                            libraryBentoTile(title: t("menu.downloads"), icon: "arrow.down.circle.fill", tint: FluoThemeTokens.amber) {
                                openSheet(.downloads)
                            }
                            libraryBentoTile(title: t("menu.feedback"), icon: "ladybug.fill", tint: FluoThemeTokens.indigo) {
                                openSheet(.feedback)
                            }
                            libraryBentoTile(title: t("menu.settings"), icon: "gearshape.fill", tint: FluoThemeTokens.coral) {
                                openSheet(.settings)
                            }
                        }

                        // Contextual Footer Pills
                        LazyVGrid(columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)], spacing: 8) {
                            if !store.activeTab.isStartPage {
                                footerPill(title: t("menu.startPage"), icon: "house") {
                                    dismissAndRun {
                                        store.activeTab.openStartPage()
                                    }
                                }
                                footerPill(title: t("sec.title"), icon: "lock.shield") {
                                    openSheet(.siteSecurity)
                                }
                            }
                            if store.activeWorkspaceTabs.contains(where: { !$0.isStartPage }) {
                                footerPill(title: t("menu.bookmarkAll"), icon: "star.square.on.square") {
                                    store.bookmarkAllTabs()
                                    closeSheet()
                                }
                            }
                            if !store.closedTabs.isEmpty {
                                footerPill(title: t("tabs.reopen"), icon: "arrow.uturn.left.circle") {
                                    dismissAndRun {
                                        store.reopenClosedTab()
                                    }
                                }
                            }
                        }
                    }
                    .transition(.opacity.combined(with: .move(edge: .top)))
                }
            }
            .padding(16)
        }
        .background { LiquidAuroraBackground().ignoresSafeArea() }
        .navigationTitle(t("menu.title"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button(t("common.close")) { closeSheet() }
            }
        }
        }
    }

    private func heroBentoLabel(title: String, icon: String, tint: Color, active: Bool, disabled: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            SpatialIconMedallion(icon: icon, tint: tint, size: 34, active: active)
            Text(title)
                .font(.caption.weight(.bold))
                .lineLimit(2)
                .multilineTextAlignment(.leading)
                .foregroundStyle(active ? .white : .primary)
        }
        .frame(maxWidth: .infinity, minHeight: 82, alignment: .topLeading)
        .padding(14)
        .liquidGlass(
            cornerRadius: 22,
            tint: active ? tint : .clear,
            interactive: false,
            prominent: active
        )
        .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
        .opacity(disabled ? 0.42 : 1.0)
    }

    private func heroBentoTile(title: String, icon: String, tint: Color, active: Bool, disabled: Bool = false, action: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            action()
        } label: {
            heroBentoLabel(title: title, icon: icon, tint: tint, active: active, disabled: disabled)
        }
        .buttonStyle(LiquidPressButtonStyle())
        .disabled(disabled)
    }

    private func compactTogglePill(title: String, icon: String, active: Bool, action: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            action()
        } label: {
            VStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.subheadline.weight(.bold))
                Text(title)
                    .font(.system(size: 10, weight: .bold))
                    .lineLimit(1)
            }
            .foregroundStyle(active ? .white : .primary)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
            .padding(.horizontal, 6)
            .liquidGlass(
                cornerRadius: 14,
                tint: active ? FluoThemeTokens.indigo : .clear,
                interactive: false,
                prominent: active
            )
            .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
    }

    private func libraryBentoTile(title: String, icon: String, tint: Color, action: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            action()
        } label: {
            HStack(spacing: 10) {
                SpatialIconMedallion(icon: icon, tint: tint, size: 32)
                Text(title)
                    .font(.caption.weight(.bold))
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                Spacer(minLength: 0)
            }
            .padding(12)
            .frame(maxWidth: .infinity, minHeight: 58, alignment: .leading)
            .liquidGlass(cornerRadius: 20, interactive: false)
            .contentShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
    }

    private func footerPill(title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 7) {
                Image(systemName: icon)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.secondary)
                Text(title)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .liquidGlass(cornerRadius: 16, interactive: false)
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(LiquidPressButtonStyle())
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

    private var folderSuggestions: [String] {
        var list = ["Default", "Reading List"]
        for item in store.userData.bookmarks {
            let folder = item.folder.trimmingCharacters(in: .whitespacesAndNewlines)
            if !folder.isEmpty && !list.contains(folder) {
                list.append(folder)
            }
        }
        return list
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        Label(t("bm.nameLabel"), systemImage: "textformat")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.secondary)
                        TextField(t("bm.nameLabel"), text: $nameDraft)
                            .font(.body)
                    }
                    .padding(.vertical, 2)

                    VStack(alignment: .leading, spacing: 6) {
                        Label(t("bm.urlLabel"), systemImage: "link")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.secondary)
                        TextField("https://example.com", text: $urlDraft)
                            .font(.subheadline)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.URL)
                    }
                    .padding(.vertical, 2)

                    VStack(alignment: .leading, spacing: 8) {
                        Label(t("bm.folderLabel"), systemImage: "folder")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.secondary)
                        TextField(t("bm.folderLabel"), text: $folderDraft)
                            .font(.body)

                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 6) {
                                ForEach(folderSuggestions, id: \.self) { folder in
                                    let isSelected = folderDraft.trimmingCharacters(in: .whitespacesAndNewlines) == folder
                                    Button {
                                        folderDraft = folder
                                    } label: {
                                        Text(folder)
                                            .font(.caption.weight(.bold))
                                            .foregroundStyle(isSelected ? .white : .primary)
                                            .padding(.horizontal, 11)
                                            .padding(.vertical, 6)
                                            .liquidGlassPill(
                                                tint: isSelected ? FluoThemeTokens.indigo : .clear,
                                                interactive: false,
                                                prominent: isSelected
                                            )
                                            .contentShape(Capsule())
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                            .padding(.vertical, 2)
                        }
                    }
                    .padding(.vertical, 2)
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
            .background { LiquidAuroraBackground().ignoresSafeArea() }
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
                            .disabled(doNotTranslate)
                            .opacity(doNotTranslate ? 0.5 : 1.0)
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
                                                    Text(t("gl.exact")).font(.caption2).foregroundStyle(.secondary)
                                                }
                                                if item.useRegex {
                                                    Text(t("gl.regex")).font(.caption2).foregroundStyle(.secondary)
                                                }
                                            }
                                            Text(item.doNotTranslate || item.target.isEmpty ? t("gl.dnt") : item.target)
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
                                        if editingId == item.id {
                                            editingId = nil
                                            src = ""
                                            dst = ""
                                            exactMatch = false
                                            useRegex = false
                                            doNotTranslate = false
                                            noteDraft = ""
                                        }
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
