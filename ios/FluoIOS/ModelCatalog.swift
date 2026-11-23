import CryptoKit
import Foundation

struct HyModelSpec: Identifiable, Hashable {
    let id: String
    let displayName: String
    let repo: String
    let expectedBytes: Int64
    let sha256: String
    let subtitle: String
    let recommendedForFreeSigning: Bool

    var downloadURL: URL {
        URL(string: "https://huggingface.co/\(repo)/resolve/main/\(id)?download=true")!
    }
}

@MainActor
final class ModelCatalog: NSObject, ObservableObject {
    static let shared = ModelCatalog()

    static let specs: [HyModelSpec] = [
        HyModelSpec(
            id: "Hy-MT2-1.8B-Q4_K_M.gguf",
            displayName: "Hy-MT2 1.8B (Q4_K_M · Recommended)",
            repo: "tencent/Hy-MT2-1.8B-GGUF",
            expectedBytes: 1_133_080_448,
            sha256: "dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53af84c4b89fb0c06699",
            subtitle: "~1.05 GB · Standard GGUF · Balanced speed & high translation accuracy",
            recommendedForFreeSigning: true
        ),
        HyModelSpec(
            id: "Hy-MT2-1.8B-Q6_K.gguf",
            displayName: "Hy-MT2 1.8B (Q6_K · High Precision)",
            repo: "tencent/Hy-MT2-1.8B-GGUF",
            expectedBytes: 1_474_785_120,
            sha256: "d98fe604dec1f28f58f80d7d560f7177e584d3b8e5835862687660e5ff97cb40",
            subtitle: "~1.37 GB · Standard GGUF · Highest nuance accuracy (recommended for 8 GB Pro iPhones)",
            recommendedForFreeSigning: false
        ),
    ]

    @Published var downloadProgress: [String: Double] = [:]
    @Published var statusError: String?

    private var activeTasks: [String: URLSessionDownloadTask] = [:]
    private var taskModelIds: [Int: String] = [:]
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 60
        config.timeoutIntervalForResource = 3600
        return URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }()

    var modelsDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("FluoModels", isDirectory: true)
        if !FileManager.default.fileExists(atPath: dir.path) {
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        var res = URLResourceValues()
        res.isExcludedFromBackup = true
        var mutableDir = dir
        try? mutableDir.setResourceValues(res)
        return dir
    }

    func filePath(for id: String) -> URL {
        modelsDirectory.appendingPathComponent(id)
    }

    func isInstalled(_ id: String) -> Bool {
        FileManager.default.fileExists(atPath: filePath(for: id).path)
    }

    func startDownload(_ spec: HyModelSpec) {
        guard activeTasks[spec.id] == nil else { return }
        statusError = nil
        downloadProgress[spec.id] = 0.01
        let task = session.downloadTask(with: spec.downloadURL)
        activeTasks[spec.id] = task
        taskModelIds[task.taskIdentifier] = spec.id
        task.resume()
    }

    func cancelDownload(_ id: String) {
        activeTasks[id]?.cancel()
        activeTasks.removeValue(forKey: id)
        downloadProgress.removeValue(forKey: id)
    }

    func deleteModel(_ id: String) {
        if id == TranslationEngine.shared.activeLocalModelPath?.lastPathComponent {
            TranslationEngine.shared.unloadOnDeviceModel()
        }
        try? FileManager.default.removeItem(at: filePath(for: id))
        objectWillChange.send()
    }
}

extension ModelCatalog: URLSessionDownloadDelegate {
    nonisolated func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        Task { @MainActor in
            guard let id = taskModelIds[downloadTask.taskIdentifier] else { return }
            let expected = totalBytesExpectedToWrite > 0
                ? totalBytesExpectedToWrite
                : (Self.specs.first(where: { $0.id == id })?.expectedBytes ?? 1)
            downloadProgress[id] = min(0.99, Double(totalBytesWritten) / Double(expected))
        }
    }

    nonisolated func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        let taskId = downloadTask.taskIdentifier
        if let http = downloadTask.response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            Task { @MainActor in
                if let id = taskModelIds.removeValue(forKey: taskId) {
                    activeTasks.removeValue(forKey: id)
                    downloadProgress.removeValue(forKey: id)
                }
                statusError = "Model download failed (HTTP \(http.statusCode))"
            }
            return
        }
        let tmpCopy = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".gguf")
        do {
            try FileManager.default.moveItem(at: location, to: tmpCopy)
        } catch {
            Task { @MainActor in
                statusError = error.localizedDescription
            }
            return
        }

        Task { @MainActor in
            defer {
                if let id = taskModelIds.removeValue(forKey: taskId) {
                    activeTasks.removeValue(forKey: id)
                    downloadProgress.removeValue(forKey: id)
                }
            }
            guard let id = taskModelIds[taskId],
                  let spec = Self.specs.first(where: { $0.id == id }) else {
                try? FileManager.default.removeItem(at: tmpCopy)
                return
            }

            // Stream SHA-256 verification on background utility queue
            let verified = await Task.detached(priority: .utility) { () -> Bool in
                guard let handle = try? FileHandle(forReadingFrom: tmpCopy) else { return false }
                defer { try? handle.close() }
                var hasher = SHA256()
                while true {
                    let chunk = handle.readData(ofLength: 1024 * 1024)
                    if chunk.isEmpty { break }
                    hasher.update(data: chunk)
                }
                let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
                return digest.lowercased() == spec.sha256.lowercased()
            }.value

            guard verified else {
                try? FileManager.default.removeItem(at: tmpCopy)
                statusError = "SHA-256 checksum mismatch for \(spec.id)"
                return
            }

            let dest = filePath(for: spec.id)
            try? FileManager.default.removeItem(at: dest)
            do {
                try FileManager.default.moveItem(at: tmpCopy, to: dest)
                UserDefaults.standard.set(spec.id, forKey: "fluo.selectedModel")
                objectWillChange.send()
            } catch {
                try? FileManager.default.removeItem(at: tmpCopy)
                statusError = error.localizedDescription
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let error else { return }
        let nsErr = error as NSError
        if nsErr.domain == NSURLErrorDomain && nsErr.code == NSURLErrorCancelled { return }
        let taskId = task.taskIdentifier
        Task { @MainActor in
            if let id = taskModelIds.removeValue(forKey: taskId) {
                activeTasks.removeValue(forKey: id)
                downloadProgress.removeValue(forKey: id)
            }
            statusError = error.localizedDescription
        }
    }
}
