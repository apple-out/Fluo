import SwiftUI

@main
struct FluoIOSApp: App {
    @StateObject private var store = BrowserStore()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ContentView(store: store)
                .onChange(of: scenePhase) { phase in
                    if phase == .background {
                        store.cancelTranslation()
                        store.persistUserData()
                        // Release GGUF / Metal RAM buffers immediately to avoid iOS Jetsam kills.
                        TranslationEngine.shared.unloadOnDeviceModel()
                    } else if phase == .inactive {
                        store.persistUserData()
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.didReceiveMemoryWarningNotification)) { _ in
                    store.sleepInactiveTabs(keepCount: 2)
                    if !store.isTranslating {
                        TranslationEngine.shared.unloadOnDeviceModel()
                    }
                }
        }
    }
}
