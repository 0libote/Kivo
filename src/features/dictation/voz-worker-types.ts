export type VozModelPhase = "downloading" | "preparing" | "ready" | "notDownloaded" | "failed";

export interface VozTranscript {
  text: string;
  words: { text: string; start: number; end: number }[];
  durationSeconds: number;
  processingSeconds: number;
  detectedLanguage: string | null;
  languageReliable: boolean;
  languageConfidence: number;
}

export interface VozWorkerRequest {
  requestId: string;
  operation: "status" | "download" | "prepare" | "remove" | "transcribe";
  samples?: number[];
  language?: string;
}

export interface VozWorkerReply {
  requestId: string;
  complete: boolean;
  phase?: VozModelPhase;
  progress?: number | null;
  downloaded?: boolean;
  transcript?: VozTranscript;
  error?: string;
}
