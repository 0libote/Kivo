import type { Ear as EarModel } from "@desert-ant-labs/ear";
import type { Voz as VozModel } from "@desert-ant-labs/voz";
import type {
  VozModelPhase,
  VozTranscript,
  VozWorkerReply,
  VozWorkerRequest,
} from "./voz-worker-types";

let recognizer: VozModel | null = null;
let loadPromise: Promise<VozModel> | null = null;
let identifier: EarModel | null = null;
const stateCache = "kivo-voz-state";

async function setLanguageCheckReady(ready: boolean) {
  const cache = await caches.open(stateCache);
  const key = new Request(new URL("/voz-language-check-ready", self.location.origin));
  if (ready) await cache.put(key, new Response("ready"));
  else await cache.delete(key);
}

function send(reply: VozWorkerReply) {
  // Web Worker postMessage has no targetOrigin argument (unlike Window.postMessage).
  // oxlint-disable-next-line unicorn/require-post-message-target-origin
  self.postMessage(reply);
}

function progress(requestId: string, phase: VozModelPhase, amount: number | null = null) {
  send({ requestId, complete: false, phase, progress: amount });
}

async function loadEar(requestId: string) {
  if (identifier) return identifier;
  const { Ear } = await import("@desert-ant-labs/ear");
  progress(requestId, "downloading");
  identifier = await Ear.load();
  await setLanguageCheckReady(true);
  return identifier;
}

async function load(requestId: string, needsDownload: boolean) {
  if (recognizer) return recognizer;
  if (loadPromise) return loadPromise;
  loadPromise = (async () => {
    const { Voz } = await import("@desert-ant-labs/voz");
    if (needsDownload) progress(requestId, "downloading");
    const loaded = await Voz.load({
      onProgress: (fraction: number) => progress(requestId, "downloading", fraction),
    });
    progress(requestId, "preparing");
    recognizer = loaded;
    return recognizer;
  })();
  try {
    return await loadPromise;
  } finally {
    loadPromise = null;
  }
}

self.addEventListener("message", (event: MessageEvent<unknown>) => {
  // The worker is only intended to accept requests from Kivo's own WebView.
  // Reject cross-origin messages before reading or acting on their payload.
  if (event.origin !== self.location.origin) return;
  if (!isVozWorkerRequest(event.data)) return;
  void handle(event.data);
});

function isVozWorkerRequest(value: unknown): value is VozWorkerRequest {
  if (typeof value !== "object" || value === null) return false;
  const request = value as Partial<VozWorkerRequest>;
  return (
    typeof request.requestId === "string" &&
    ["status", "download", "prepare", "remove", "transcribe"].includes(request.operation as string)
  );
}

async function handle(request: VozWorkerRequest) {
  try {
    switch (request.operation) {
      case "status": {
        const cacheNames = typeof caches === "undefined" ? [] : await caches.keys();
        const readyCache = await caches.open(stateCache);
        const languageCheckInstalled = await readyCache.match(
          new Request(new URL("/voz-language-check-ready", self.location.origin)),
        );
        const downloaded =
          cacheNames.some((name) => name.startsWith("desert-ant-voz-")) &&
          languageCheckInstalled !== undefined;
        send({
          requestId: request.requestId,
          complete: true,
          phase: downloaded ? "ready" : "notDownloaded",
          progress: null,
          downloaded,
        });
        return;
      }
      case "download":
      case "prepare": {
        await loadEar(request.requestId);
        const alreadyCached = await caches
          .keys()
          .then((names) => names.some((name) => name.startsWith("desert-ant-voz-")));
        const loaded = await load(
          request.requestId,
          request.operation === "download" || !alreadyCached,
        );
        if (!loaded) throw new Error("Voz could not be loaded.");
        send({
          requestId: request.requestId,
          complete: true,
          phase: "ready",
          progress: null,
          downloaded: true,
        });
        return;
      }
      case "remove": {
        recognizer = null;
        identifier?.dispose();
        identifier = null;
        await setLanguageCheckReady(false);
        const names = await caches.keys();
        await Promise.all(
          names
            .filter((name) => name.startsWith("desert-ant-voz-"))
            .map((name) => caches.delete(name)),
        );
        send({
          requestId: request.requestId,
          complete: true,
          phase: "notDownloaded",
          progress: null,
          downloaded: false,
        });
        return;
      }
      case "transcribe": {
        if (!request.samples || !request.language) {
          throw new Error("Voz needs captured audio and a selected language.");
        }
        const ear = await loadEar(request.requestId);
        const loaded = await load(request.requestId, false);
        if (!loaded)
          throw new Error("Voz is not prepared. Download it in Dictation settings first.");
        const samples = Float32Array.from(request.samples);
        const detection = await ear.identify(samples, 16000);
        const supported =
          "bg cs da de el en es et fi fr hr hu it lt lv mt nl pl pt ro ru sk sl sv uk".split(" ");
        if (!detection.isReliable || !detection.language) {
          throw new Error(
            "Kivo could not reliably identify the spoken language. Choose another engine or set a clearer language selection.",
          );
        }
        if (!supported.includes(detection.language)) {
          throw new Error(
            `Voz does not support the detected language (${detection.language}). Choose System or Kivo On-device.`,
          );
        }
        const selected = request.language.split(/[-_]/, 1)[0]?.toLowerCase();
        if (request.language !== "auto" && selected !== detection.language.toLowerCase()) {
          throw new Error(
            `The audio sounds like ${detection.language}, but Kivo is set to ${selected}. Change the dictation language or choose another engine.`,
          );
        }
        const raw = await loaded.transcribe(samples);
        const transcript: VozTranscript = {
          text: raw.text,
          words: raw.words,
          durationSeconds: raw.duration,
          processingSeconds: raw.processingTime,
          detectedLanguage: detection.language,
          languageReliable: detection.isReliable,
          languageConfidence: detection.confidence,
        };
        send({ requestId: request.requestId, complete: true, transcript });
        return;
      }
      default:
        throw new Error("Unsupported Voz worker operation.");
    }
  } catch (cause) {
    const message =
      cause instanceof Error ? cause.message : "Voz could not complete this operation.";
    send({ requestId: request.requestId, complete: true, phase: "failed", error: message });
  }
}
