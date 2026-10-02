import type { Ear as EarModel } from "@desert-ant-labs/ear";
import type { Voz as VozModel } from "@desert-ant-labs/voz";
import type {
  VozModelPhase,
  VozTranscript,
  VozWorkerReply,
  VozWorkerRequest,
} from "./voz-worker-types";

// Keep the Windows installer free of the browser ML stack. These exact,
// version-pinned modules are fetched only when the user installs/prepares Voz.
// jsDelivr serves immutable npm versions, and the model weights themselves
// remain pinned/verified by Desert Ant's SDK.
const EAR_SDK_URL = "https://cdn.jsdelivr.net/npm/@desert-ant-labs/ear@3.5.0/+esm";
const VOZ_SDK_URL = "https://cdn.jsdelivr.net/npm/@desert-ant-labs/voz@3.5.0/+esm";
const LITERT_URL = "https://cdn.jsdelivr.net/npm/@litertjs/core@2.5.3/+esm";
const LITERT_WASM_DIR = "https://cdn.jsdelivr.net/npm/@litertjs/core@2.5.3/wasm/";
const ORT_WEBGPU_URL =
  "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.30.0/dist/ort.webgpu.bundle.min.mjs";

let recognizer: VozModel | null = null;
let loadPromise: Promise<VozModel> | null = null;
let identifier: EarModel | null = null;
let earRuntimePromise: Promise<{
  Ear: typeof import("@desert-ant-labs/ear").Ear;
  litert: unknown;
}> | null = null;
let vozRuntimePromise: Promise<{
  Voz: typeof import("@desert-ant-labs/voz").Voz;
  ort: unknown;
}> | null = null;

const stateCache = "kivo-voz-state";
const languageCheckReadyPath = "/voz-language-check-ready";
const vozModelReadyPath = "/voz-model-ready";

function stateMarker(path: string) {
  return new Request(new URL(path, self.location.origin));
}

async function setReadyMarker(path: string, ready: boolean) {
  const cache = await caches.open(stateCache);
  const key = stateMarker(path);
  if (ready) await cache.put(key, new Response("ready"));
  else await cache.delete(key);
}

async function setLanguageCheckReady(ready: boolean) {
  await setReadyMarker(languageCheckReadyPath, ready);
}

async function setVozModelReady(ready: boolean) {
  await setReadyMarker(vozModelReadyPath, ready);
}

function send(reply: VozWorkerReply) {
  // Web Worker postMessage has no targetOrigin argument (unlike Window.postMessage).
  // oxlint-disable-next-line unicorn/require-post-message-target-origin
  self.postMessage(reply);
}

function progress(requestId: string, phase: VozModelPhase, amount: number | null = null) {
  send({ requestId, complete: false, phase, progress: amount });
}

async function importRemote<T>(url: string): Promise<T> {
  try {
    // The URL is deliberately opaque to Vite: following it at build time would
    // put the Voz/Ear/ONNX/LiteRT chunks straight back into the installer.
    return (await import(/* @vite-ignore */ url)) as T;
  } catch (cause) {
    throw new Error(
      "Kivo could not download the Windows Voz runtime. Check your internet connection and retry.",
      { cause },
    );
  }
}

async function loadEarRuntime(requestId: string) {
  if (!earRuntimePromise) {
    progress(requestId, "downloading");
    earRuntimePromise = Promise.all([
      importRemote<typeof import("@desert-ant-labs/ear")>(EAR_SDK_URL),
      importRemote<typeof import("@litertjs/core")>(LITERT_URL),
    ])
      .then(([ear, litert]) => ({ Ear: ear.Ear, litert }))
      .catch((error) => {
        earRuntimePromise = null;
        throw error;
      });
  }
  return earRuntimePromise;
}

async function loadVozRuntime(requestId: string) {
  if (!vozRuntimePromise) {
    progress(requestId, "downloading");
    vozRuntimePromise = Promise.all([
      importRemote<typeof import("@desert-ant-labs/voz")>(VOZ_SDK_URL),
      importRemote<unknown>(ORT_WEBGPU_URL),
    ])
      .then(([voz, ort]) => ({ Voz: voz.Voz, ort }))
      .catch((error) => {
        vozRuntimePromise = null;
        throw error;
      });
  }
  return vozRuntimePromise;
}

async function loadEar(requestId: string) {
  if (identifier) return identifier;
  const { Ear, litert } = await loadEarRuntime(requestId);
  identifier = await Ear.load({
    litert,
    litertWasmDir: LITERT_WASM_DIR,
    onProgress: (fraction: number) => progress(requestId, "downloading", fraction),
  });
  await setLanguageCheckReady(true);
  return identifier;
}

async function load(requestId: string, needsDownload: boolean) {
  if (recognizer) return recognizer;
  if (loadPromise) return loadPromise;
  loadPromise = (async () => {
    const { Voz, ort } = await loadVozRuntime(requestId);
    if (needsDownload) progress(requestId, "downloading");
    const loaded = await Voz.load({
      ort,
      onProgress: (fraction: number) => progress(requestId, "downloading", fraction),
    });
    progress(requestId, "preparing");
    await setVozModelReady(true);
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
  // Dedicated-worker MessageEvents can expose either the app origin or an empty
  // origin depending on the WebView2 messaging path. Reject any other populated
  // origin, then validate the structured request before doing work.
  if (event.origin !== "" && event.origin !== self.location.origin) return;
  if (!isVozWorkerRequest(event.data)) return;
  void handle(event.data);
});

// Workers do not support a targetOrigin argument on postMessage.
// oxlint-disable-next-line unicorn/require-post-message-target-origin
self.postMessage({ type: "ready" });

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
      case "status":
        await reportStatus(request.requestId);
        return;
      case "download":
      case "prepare":
        await install(request);
        return;
      case "remove":
        await remove(request.requestId);
        return;
      case "transcribe":
        await transcribe(request);
        return;
      default:
        throw new Error("Unsupported Voz worker operation.");
    }
  } catch (cause) {
    const message =
      cause instanceof Error ? cause.message : "Voz could not complete this operation.";
    send({ requestId: request.requestId, complete: true, phase: "failed", error: message });
  }
}

async function reportStatus(requestId: string) {
  const readyCache = await caches.open(stateCache);
  const [vozInstalled, languageCheckInstalled] = await Promise.all([
    readyCache.match(stateMarker(vozModelReadyPath)),
    readyCache.match(stateMarker(languageCheckReadyPath)),
  ]);
  const downloaded = vozInstalled !== undefined && languageCheckInstalled !== undefined;
  send({
    requestId,
    complete: true,
    phase: downloaded ? "ready" : "notDownloaded",
    progress: null,
    downloaded,
  });
}

async function install(request: VozWorkerRequest) {
  await loadEar(request.requestId);
  const loaded = await load(request.requestId, request.operation === "download");
  if (!loaded) throw new Error("Voz could not be loaded.");
  send({
    requestId: request.requestId,
    complete: true,
    phase: "ready",
    progress: null,
    downloaded: true,
  });
}

async function remove(requestId: string) {
  recognizer = null;
  identifier?.dispose();
  identifier = null;
  await Promise.all([setLanguageCheckReady(false), setVozModelReady(false)]);
  const names = await caches.keys();
  await Promise.all(
    names.filter((name) => name.startsWith("desert-ant-voz-")).map((name) => caches.delete(name)),
  );
  send({
    requestId,
    complete: true,
    phase: "notDownloaded",
    progress: null,
    downloaded: false,
  });
}

async function transcribe(request: VozWorkerRequest) {
  if (!request.samples || !request.language) {
    throw new Error("Voz needs captured audio and a selected language.");
  }
  const ear = await loadEar(request.requestId);
  const loaded = await load(request.requestId, false);
  if (!loaded) throw new Error("Voz is not prepared. Download it in Dictation settings first.");
  const samples = Float32Array.from(request.samples);
  const detection = await ear.identify(samples, 16000);
  assertSupportedLanguage(request.language, detection);
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
}

function assertSupportedLanguage(
  selectedLanguage: string,
  detection: Awaited<ReturnType<EarModel["identify"]>>,
) {
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
  const selected = selectedLanguage.split(/[-_]/, 1)[0]?.toLowerCase();
  if (selectedLanguage !== "auto" && selected !== detection.language.toLowerCase()) {
    throw new Error(
      `The audio sounds like ${detection.language}, but Kivo is set to ${selected}. Change the dictation language or choose another engine.`,
    );
  }
}
