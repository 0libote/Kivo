import AVFoundation
import Foundation
import Speech

public typealias KivoSpeechCallback = @convention(c) (
    UnsafeMutableRawPointer?,
    Int32,
    UnsafePointer<CChar>?,
    Float
) -> Void

@available(macOS 26.0, *)
private final class SpeechSession: @unchecked Sendable {
    private let callback: KivoSpeechCallback
    private let context: UnsafeMutableRawPointer?
    private let engine = AVAudioEngine()
    private let analyzer: SpeechAnalyzer
    private let transcriber: DictationTranscriber
    private let inputStream: AsyncStream<AnalyzerInput>
    private let inputContinuation: AsyncStream<AnalyzerInput>.Continuation
    private let lock = NSLock()
    private var analysisTask: Task<Void, Never>?
    private var resultsTask: Task<Void, Never>?
    private var finalSegments: [String] = []
    private var volatileSegment = ""
    private var started = false
    private var stopRequested = false
    private var cancelled = false

    init?(
        localeIdentifier: String?,
        requireOnDevice: Bool,
        callback: @escaping KivoSpeechCallback,
        context: UnsafeMutableRawPointer?
    ) {
        self.callback = callback
        self.context = context

        guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized,
              SFSpeechRecognizer.authorizationStatus() == .authorized else {
            return nil
        }

        guard requireOnDevice else { return nil }
        let locale = localeIdentifier.map(Locale.init(identifier:)) ?? .current
        let transcriber = DictationTranscriber(locale: locale, preset: .progressiveShortDictation)
        self.transcriber = transcriber
        self.analyzer = SpeechAnalyzer(
            modules: [transcriber],
            options: .init(priority: .userInitiated, modelRetention: .lingering)
        )
        let stream = AsyncStream<AnalyzerInput>.makeStream()
        self.inputStream = stream.stream
        self.inputContinuation = stream.continuation

        analysisTask = Task { [weak self] in
            await self?.prepareAndStart()
        }
    }

    func stop() {
        lock.lock()
        guard !cancelled, !stopRequested else {
            lock.unlock()
            return
        }
        stopRequested = true
        let wasStarted = started
        lock.unlock()

        if wasStarted {
            stopAudio()
            inputContinuation.finish()
            Task { [weak self] in
                guard let self else { return }
                do {
                    try await analyzer.finalizeAndFinishThroughEndOfInput()
                } catch {
                    emit(4, "On-device transcription could not be finalized.")
                }
            }
        }
    }

    func cancel() {
        lock.lock()
        guard !cancelled else {
            lock.unlock()
            return
        }
        cancelled = true
        let wasStarted = started
        lock.unlock()

        if wasStarted { stopAudio() }
        inputContinuation.finish()
        resultsTask?.cancel()
        Task { [weak self] in
            guard let self else { return }
            await analyzer.cancelAndFinishNow()
        }
    }

    private func prepareAndStart() async {
        let modules: [any SpeechModule] = [transcriber]
        let assetStatus = await AssetInventory.status(forModules: modules)
        guard assetStatus == .installed else {
            emit(4, "The on-device speech model for this language is not installed.")
            return
        }

        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            emit(4, "The selected microphone is unavailable.")
            return
        }

        do {
            try await analyzer.prepareToAnalyze(in: format)
            try await analyzer.start(inputSequence: inputStream)

            let (shouldStop, wasCancelled) = lock.withLock {
                let shouldStop = stopRequested || cancelled
                started = !shouldStop
                return (shouldStop, cancelled)
            }
            guard !shouldStop else {
                inputContinuation.finish()
                await analyzer.cancelAndFinishNow()
                if !wasCancelled { emit(2, "") }
                return
            }

            resultsTask = Task { [weak self] in
                await self?.consumeResults()
            }
            input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
                guard let self, self.acceptingAudio else { return }
                inputContinuation.yield(AnalyzerInput(buffer: buffer))
                emitLevel(buffer)
            }
            engine.prepare()
            try engine.start()
            emit(0, nil)
        } catch {
            if engine.inputNode.numberOfInputs > 0 {
                engine.inputNode.removeTap(onBus: 0)
            }
            emit(4, "On-device speech recognition could not be started.")
        }
    }

    private func consumeResults() async {
        do {
            for try await result in transcriber.results {
                let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                let (snapshot, shouldEmit) = lock.withLock {
                    if result.isFinal {
                        if !text.isEmpty { finalSegments.append(text) }
                        volatileSegment = ""
                    } else {
                        volatileSegment = text
                    }
                    let snapshot = (finalSegments + (volatileSegment.isEmpty ? [] : [volatileSegment]))
                        .joined(separator: " ")
                    return (snapshot, !cancelled)
                }
                if shouldEmit && !snapshot.isEmpty { emit(1, snapshot) }
            }

            let (finalText, shouldEmit) = lock.withLock {
                (finalSegments.joined(separator: " "), !cancelled)
            }
            if shouldEmit { emit(2, finalText) }
        } catch {
            let shouldEmit = lock.withLock { !cancelled }
            if shouldEmit { emit(4, "On-device transcription stopped unexpectedly.") }
        }
    }

    private var acceptingAudio: Bool {
        lock.lock()
        defer { lock.unlock() }
        return started && !stopRequested && !cancelled
    }

    private func stopAudio() {
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
    }

    private func emit(_ event: Int32, _ text: String?) {
        if let text {
            text.withCString { callback(context, event, $0, 0) }
        } else {
            callback(context, event, nil, 0)
        }
    }

    private func emitLevel(_ buffer: AVAudioPCMBuffer) {
        guard let channels = buffer.floatChannelData, buffer.frameLength > 0 else { return }
        let samples = channels[0]
        let count = Int(buffer.frameLength)
        var sum: Float = 0
        for index in 0..<count {
            sum += samples[index] * samples[index]
        }
        let rms = sqrt(sum / Float(count))
        let normalized = max(0, min(1, (20 * log10(max(rms, 0.000_01)) + 55) / 55))
        callback(context, 3, nil, normalized)
    }
}

@_cdecl("kivo_microphone_authorization_status")
public func kivoMicrophoneAuthorizationStatus() -> Int32 {
    Int32(AVCaptureDevice.authorizationStatus(for: .audio).rawValue)
}

@_cdecl("kivo_speech_authorization_status")
public func kivoSpeechAuthorizationStatus() -> Int32 {
    Int32(SFSpeechRecognizer.authorizationStatus().rawValue)
}

@_cdecl("kivo_request_microphone_authorization")
public func kivoRequestMicrophoneAuthorization() {
    // Intentional no-op callback: status is polled via kivo_microphone_authorization_status.
    AVCaptureDevice.requestAccess(for: .audio) { _ in }
}

@_cdecl("kivo_request_speech_authorization")
public func kivoRequestSpeechAuthorization() {
    // Intentional no-op callback: status is polled via kivo_speech_authorization_status.
    SFSpeechRecognizer.requestAuthorization { _ in }
}

@_cdecl("kivo_speech_start")
public func kivoSpeechStart(
    _ locale: UnsafePointer<CChar>?,
    _ requireOnDevice: Bool,
    _ callback: KivoSpeechCallback?,
    _ context: UnsafeMutableRawPointer?
) -> UnsafeMutableRawPointer? {
    guard let callback else { return nil }
    let localeIdentifier = locale.map { String(cString: $0) }
    guard let session = SpeechSession(
        localeIdentifier: localeIdentifier,
        requireOnDevice: requireOnDevice,
        callback: callback,
        context: context
    ) else { return nil }
    return Unmanaged.passRetained(session).toOpaque()
}

@_cdecl("kivo_speech_stop")
public func kivoSpeechStop(_ handle: UnsafeMutableRawPointer?) {
    guard let handle else { return }
    Unmanaged<SpeechSession>.fromOpaque(handle).takeUnretainedValue().stop()
}

@_cdecl("kivo_speech_cancel")
public func kivoSpeechCancel(_ handle: UnsafeMutableRawPointer?) {
    guard let handle else { return }
    Unmanaged<SpeechSession>.fromOpaque(handle).takeUnretainedValue().cancel()
}

@_cdecl("kivo_speech_destroy")
public func kivoSpeechDestroy(_ handle: UnsafeMutableRawPointer?) {
    guard let handle else { return }
    Unmanaged<SpeechSession>.fromOpaque(handle).release()
}
