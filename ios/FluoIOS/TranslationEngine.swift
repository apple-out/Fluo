import Foundation

enum AiProviderKind: String, CaseIterable, Identifiable, Codable {
    case hyMt2 = "hyMt2"
    case googleGemini = "googleGemini"
    case openAiCompatible = "openAiCompatible"

    var id: String { rawValue }
}

final class TranslationEngine {
    static let shared = TranslationEngine()
    private(set) var activeLocalModelPath: URL?
    private let lock = NSLock()

    func ensureLocalModelLoaded(path: URL) throws {
        lock.lock()
        defer { lock.unlock() }
        if activeLocalModelPath == path { return }
        let code = path.path.withCString { cPath in
            fluo_load_model(cPath, 99, 1024)
        }
        guard code == 0 else {
            var detail = ""
            if let errPtr = fluo_last_error() {
                detail = String(cString: errPtr)
                fluo_free_string(errPtr)
            }
            let suffix = detail.isEmpty ? "" : ": \(detail)"
            throw NSError(
                domain: "FluoInference",
                code: Int(code),
                userInfo: [NSLocalizedDescriptionKey: "Failed to load on-device model (code \(code))\(suffix)"]
            )
        }
        activeLocalModelPath = path
    }

    func unloadOnDeviceModel() {
        lock.lock()
        defer { lock.unlock() }
        fluo_unload_model()
        activeLocalModelPath = nil
    }

    static func matchingGlossaryHint(for source: String, glossary: [GlossaryItem]) -> String {
        guard !glossary.isEmpty else { return "" }
        let matched = glossary.filter { item in
            let needle = item.source.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !needle.isEmpty else { return false }
            if item.caseSensitive {
                return source.contains(needle)
            }
            return source.localizedCaseInsensitiveContains(needle)
        }
        guard !matched.isEmpty else { return "" }
        return matched.map { item in
            let target = item.target.trimmingCharacters(in: .whitespacesAndNewlines)
            if target.isEmpty {
                return "\(item.source) -> \(item.source) (keep original)"
            }
            return "\(item.source) -> \(target)"
        }.joined(separator: "\n")
    }

    func translateText(
        _ source: String,
        provider: AiProviderKind,
        modelId: String,
        baseUrl: String,
        targetLanguage: String,
        style: String = "auto",
        glossary: [GlossaryItem] = []
    ) async throws -> String {
        if source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return source
        }
        let glossaryHint = Self.matchingGlossaryHint(for: source, glossary: glossary)
        let glossaryBlock = glossaryHint.isEmpty ? "" : "\nGlossary (apply strictly):\n\(glossaryHint)\n"

        switch provider {
        case .hyMt2:
            let modelUrl = await ModelCatalog.shared.filePath(for: modelId)
            guard FileManager.default.fileExists(atPath: modelUrl.path) else {
                throw NSError(
                    domain: "FluoInference",
                    code: -2,
                    userInfo: [NSLocalizedDescriptionKey: "On-device model '\(modelId)' is not installed yet."]
                )
            }
            return try await Task.detached(priority: .userInitiated) {
                try self.ensureLocalModelLoaded(path: modelUrl)
                self.lock.lock()
                defer { self.lock.unlock() }
                let rawPtr = source.withCString { cSrc in
                    targetLanguage.withCString { cTarget in
                        style.withCString { cStyle in
                            glossaryHint.withCString { cGlossary in
                                fluo_translate_with_glossary(cSrc, cTarget, cStyle, cGlossary)
                            }
                        }
                    }
                }
                guard let rawPtr else {
                    throw NSError(
                        domain: "FluoInference",
                        code: -1,
                        userInfo: [NSLocalizedDescriptionKey: "Null response from inference engine."]
                    )
                }
                defer { fluo_free_string(rawPtr) }
                let jsonStr = String(cString: rawPtr)
                guard let data = jsonStr.data(using: .utf8),
                      let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    throw NSError(
                        domain: "FluoInference",
                        code: -3,
                        userInfo: [NSLocalizedDescriptionKey: "Malformed JSON response from inference engine."]
                    )
                }
                if (obj["ok"] as? Bool) == true, let tr = obj["translation"] as? String {
                    return tr
                }
                let errMsg = (obj["error"] as? String) ?? "Unknown inference error"
                throw NSError(
                    domain: "FluoInference",
                    code: -4,
                    userInfo: [NSLocalizedDescriptionKey: errMsg]
                )
            }.value

        case .googleGemini:
            guard let apiKey = KeychainStore.load(account: "gemini_api_key"), !apiKey.isEmpty else {
                throw NSError(
                    domain: "FluoCloud",
                    code: 401,
                    userInfo: [NSLocalizedDescriptionKey: "Set your Google AI Studio API key in Settings."]
                )
            }
            let modelName = modelId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "gemini-2.5-flash" : modelId
            guard let url = URL(string: "https://generativelanguage.googleapis.com/v1beta/models/\(modelName):generateContent?key=\(apiKey)") else {
                throw URLError(.badURL)
            }
            let prompt = "Translate the following passage into \(targetLanguage) using style '\(style)', keeping inline link/media tags like [[1|...]] or [[img1]] verbatim. Output ONLY the translation.\(glossaryBlock)\n\n\(source)"
            let body: [String: Any] = [
                "contents": [["parts": [["text": prompt]]]],
                "generationConfig": ["temperature": 0.2],
            ]
            var req = URLRequest(url: url)
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
            let (data, resp) = try await URLSession.shared.data(for: req)
            if let http = resp as? HTTPURLResponse, http.statusCode >= 400 {
                let errText = String(data: data, encoding: .utf8) ?? "HTTP \(http.statusCode)"
                throw NSError(domain: "FluoCloud", code: http.statusCode, userInfo: [NSLocalizedDescriptionKey: errText])
            }
            if let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let cands = root["candidates"] as? [[String: Any]],
               let content = cands.first?["content"] as? [String: Any],
               let parts = content["parts"] as? [[String: Any]] {
                let joined = parts.compactMap { $0["text"] as? String }.joined()
                if !joined.isEmpty { return joined.trimmingCharacters(in: .whitespacesAndNewlines) }
            }
            throw NSError(domain: "FluoCloud", code: -5, userInfo: [NSLocalizedDescriptionKey: "Empty Gemini response."])

        case .openAiCompatible:
            guard let apiKey = KeychainStore.load(account: "openai_api_key"), !apiKey.isEmpty else {
                throw NSError(
                    domain: "FluoCloud",
                    code: 401,
                    userInfo: [NSLocalizedDescriptionKey: "Set your OpenAI API key in Settings."]
                )
            }
            var cleanBase = baseUrl.trimmingCharacters(in: .whitespacesAndNewlines)
            if cleanBase.isEmpty { cleanBase = "https://api.openai.com/v1" }
            if cleanBase.hasSuffix("/") { cleanBase.removeLast() }
            if !cleanBase.hasSuffix("/chat/completions") { cleanBase += "/chat/completions" }
            guard let url = URL(string: cleanBase) else { throw URLError(.badURL) }

            let cleanModel = modelId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "gpt-4.1-mini" : modelId
            let body: [String: Any] = [
                "model": cleanModel,
                "temperature": 0.2,
                "messages": [
                    ["role": "system", "content": "Translate into \(targetLanguage) (style: \(style)). Preserve [[1|...]] and [[img1]] markers verbatim. Output ONLY the translated text.\(glossaryBlock)"],
                    ["role": "user", "content": source],
                ],
            ]
            var req = URLRequest(url: url)
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
            let (data, resp) = try await URLSession.shared.data(for: req)
            if let http = resp as? HTTPURLResponse, http.statusCode >= 400 {
                let errText = String(data: data, encoding: .utf8) ?? "HTTP \(http.statusCode)"
                throw NSError(domain: "FluoCloud", code: http.statusCode, userInfo: [NSLocalizedDescriptionKey: errText])
            }
            if let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let choices = root["choices"] as? [[String: Any]],
               let msg = choices.first?["message"] as? [String: Any],
               let text = msg["content"] as? String {
                return text.trimmingCharacters(in: .whitespacesAndNewlines)
            }
            throw NSError(domain: "FluoCloud", code: -6, userInfo: [NSLocalizedDescriptionKey: "Empty OpenAI response."])
        }
    }
}
