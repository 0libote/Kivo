import DesertAnt
import Ear
import Darwin
import Foundation
import Voz

public typealias ProgressCallback = @convention(c) (Double, UnsafeMutableRawPointer?) -> Void
private final class LockedBox<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Value

    init(_ value: Value) { self.value = value }
    func get() -> Value { lock.lock(); defer { lock.unlock() }; return value }
    func set(_ value: Value) { lock.lock(); defer { lock.unlock() }; self.value = value }
}

private let loadedVoz = LockedBox<Voz?>(nil)
private let loadedEar = LockedBox<Ear?>(nil)

private final class ProgressSink: @unchecked Sendable {
    private let callback: ProgressCallback?
    private let context: UnsafeMutableRawPointer?
    init(_ callback: ProgressCallback?, _ context: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.context = context
    }
    func report(_ value: Double) { callback?(value, context) }
}

private func withString(_ pointer: UnsafePointer<CChar>?) -> String? {
    pointer.map { String(cString: $0) }
}

private func jsonString(_ value: Any) -> UnsafeMutablePointer<CChar> {
    let data = (try? JSONSerialization.data(withJSONObject: value)) ?? Data("{}".utf8)
    return strdup(String(decoding: data, as: UTF8.self))!
}

@_cdecl("kivo_voz_status")
public func kivoVozStatus(_ root: UnsafePointer<CChar>?) -> Bool {
    guard let root = withString(root) else { return false }
    return Voz.isDownloaded(cacheRoot: root) && Ear.isDownloaded(cacheRoot: "\(root)/ear")
}

@_cdecl("kivo_voz_download")
public func kivoVozDownload(_ root: UnsafePointer<CChar>?, _ callback: ProgressCallback?, _ context: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar> {
    guard let root = withString(root) else { return jsonString(["error": "Invalid Voz cache directory."]) }
    let semaphore = DispatchSemaphore(value: 0)
    let failure = LockedBox<String?>(nil)
    let progressSink = ProgressSink(callback, context)
    Task {
        do {
            DesertAnt.usageDisabled = true
            let ear = Ear(directory: nil, cacheRoot: "\(root)/ear")
            try await ear.download()
            loadedEar.set(ear)
            try await Voz.download(cacheRoot: root) { progress in progressSink.report(progress.fraction) }
        } catch { failure.set(String(describing: error)) }
        semaphore.signal()
    }
    semaphore.wait()
    return jsonString(failure.get().map { ["error": $0] } ?? ["ok": true])
}

@_cdecl("kivo_voz_prepare")
public func kivoVozPrepare(_ root: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar> {
    guard let root = withString(root) else { return jsonString(["error": "Invalid Voz cache directory."]) }
    let semaphore = DispatchSemaphore(value: 0)
    let failure = LockedBox<String?>(nil)
    Task {
        do {
            DesertAnt.usageDisabled = true
            if loadedEar.get() == nil {
                let ear = Ear(directory: nil, cacheRoot: "\(root)/ear")
                try await ear.download()
                loadedEar.set(ear)
            }
            if loadedVoz.get() == nil {
                loadedVoz.set(try await Voz(cacheRoot: root))
            }
        } catch { failure.set(String(describing: error)) }
        semaphore.signal()
    }
    semaphore.wait()
    return jsonString(failure.get().map { ["error": $0] } ?? ["ok": true])
}

@_cdecl("kivo_voz_transcribe")
public func kivoVozTranscribe(_ samples: UnsafePointer<Float>?, _ count: Int, _ root: UnsafePointer<CChar>?, _ language: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar> {
    guard let samples, count > 0, let root = withString(root), let language = withString(language) else { return jsonString(["error": "Voz received no audio samples or selected language."]) }
    let input = Array(UnsafeBufferPointer(start: samples, count: count))
    let semaphore = DispatchSemaphore(value: 0)
    let resultBox = LockedBox<Any>(["error": "Voz could not complete transcription."])
    Task {
        do {
            DesertAnt.usageDisabled = true
            let ear = loadedEar.get() ?? Ear(directory: nil, cacheRoot: "\(root)/ear")
            loadedEar.set(ear)
            let detection = try await ear.identify(samples: input, sampleRate: 16_000)
            let selected = language.split(whereSeparator: { $0 == "-" || $0 == "_" }).first.map(String.init)?.lowercased() ?? language.lowercased()
            guard detection.isReliable, let detected = detection.language else {
                throw NSError(domain: "KivoVoz", code: 1, userInfo: [NSLocalizedDescriptionKey: "Kivo could not reliably identify the spoken language. Choose a language in Settings → Dictation or select another engine."])
            }
            guard Voz.supportedLanguages.contains(detected) else {
                throw NSError(domain: "KivoVoz", code: 2, userInfo: [NSLocalizedDescriptionKey: "Voz does not support the detected language (\(detected)). Choose System or Kivo On-device."])
            }
            guard selected == "auto" || selected == detected.lowercased() else {
                throw NSError(domain: "KivoVoz", code: 3, userInfo: [NSLocalizedDescriptionKey: "The audio sounds like \(detected), but Kivo is set to \(selected). Change the dictation language or choose another engine."])
            }
            let recognizer: Voz
            if let cached = loadedVoz.get() { recognizer = cached }
            else { recognizer = try await Voz(cacheRoot: root) }
            loadedVoz.set(recognizer)
            let result = try await recognizer.transcribe(samples: input)
            let detectedLanguage: Any = detection.language.map { $0 as Any } ?? NSNull()
            resultBox.set(["text": result.text, "words": result.words.map { ["text": $0.text, "start": $0.start, "end": $0.end] }, "durationSeconds": result.duration, "processingSeconds": result.processingTime, "detectedLanguage": detectedLanguage, "languageReliable": detection.isReliable, "languageConfidence": detection.confidence])
        } catch { resultBox.set(["error": String(describing: error)]) }
        semaphore.signal()
    }
    semaphore.wait()
    return jsonString(resultBox.get())
}

@_cdecl("kivo_voz_remove")
public func kivoVozRemove(_ root: UnsafePointer<CChar>?) -> Bool {
    loadedVoz.set(nil)
    loadedEar.set(nil)
    guard let root = withString(root) else { return false }
    do {
        if FileManager.default.fileExists(atPath: root) { try FileManager.default.removeItem(atPath: root) }
        return true
    } catch { return false }
}

@_cdecl("kivo_voz_free")
public func kivoVozFree(_ pointer: UnsafeMutablePointer<CChar>?) {
    free(pointer)
}
