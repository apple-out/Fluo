import SwiftUI

@main
struct FluoIOSApp: App {
    @StateObject private var store = BrowserStore()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ContentView(store: store)
                .onChange(of: scenePhase) { _, phase in
                    if phase == .background {
                        // Release GGUF / Metal RAM buffers immediately to avoid iOS Jetsam kills.
                        TranslationEngine.shared.unloadOnDeviceModel()
                    }
                }
        }
    }
}
