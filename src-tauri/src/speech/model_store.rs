//! On-device speech-model catalog and download store.
//!
//! Models are GGML/GGUF conversions published by `handy-computer` on Hugging
//! Face, spanning Whisper and every other family the `transcribe-cpp` runtime
//! supports. Every entry is pinned to a commit and verified by SHA-256 before
//! it is treated as installed, so a moved tag or a truncated download can
//! never silently become a model.

use std::{
    collections::HashMap,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

/// Default model when the setting is unset: the best size/accuracy balance for
/// a desktop CPU. See the catalog descriptions for the trade-offs.
pub const DEFAULT_LOCAL_MODEL_ID: &str = "whisper-small";

/// One verified download in the on-device catalog. `accuracy`/`speed` are
/// 0–100 comparison scores (higher is better) shown as bars, matching how
/// `transcribe.cpp` / Handy rate the same files.
struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    /// Recognizer family, e.g. "Whisper" or "Parakeet".
    family: &'static str,
    /// Parameter count label, e.g. "0.6B".
    parameters: &'static str,
    language_count: u16,
    accuracy: u8,
    speed: u8,
    streaming: bool,
    /// Hugging Face repo + the pinned commit the file was verified against.
    repo: &'static str,
    revision: &'static str,
    filename: &'static str,
    sha256: &'static str,
    size_bytes: u64,
    recommended: bool,
}

/// The catalog spans Whisper plus every other family the `transcribe-cpp`
/// runtime supports, so users can trade accuracy, speed, language coverage,
/// and size. Every entry is a GGUF published by `handy-computer` (the runtime's
/// authors) and pinned to a commit + SHA-256. Accuracy/speed are the published
/// comparison scores from `transcribe.cpp`'s catalog.
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "whisper-tiny",
        name: "Whisper Tiny",
        description: "Fastest and smallest. Good for quick notes on older machines.",
        family: "Whisper",
        parameters: "38M",
        language_count: 99,
        accuracy: 61,
        speed: 100,
        streaming: false,
        repo: "handy-computer/whisper-tiny-gguf",
        revision: "2678cc66038359b97c8e6fd6454c56fc9006d571",
        filename: "whisper-tiny-Q5_K_M.gguf",
        sha256: "72cfa8ee436a635a5b6fb373cc056a828b9efe96d32d6eb8769ed3cc5b429719",
        size_bytes: 44_211_616,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-base",
        name: "Whisper Base",
        description: "A light, responsive model for everyday dictation.",
        family: "Whisper",
        parameters: "73M",
        language_count: 99,
        accuracy: 71,
        speed: 99,
        streaming: false,
        repo: "handy-computer/whisper-base-gguf",
        revision: "30c1704968f7540e112bab3ea3cc4b274902c360",
        filename: "whisper-base-Q5_K_M.gguf",
        sha256: "8e0feb7bc35780353cf31821018e601bb7b7cff6c9a0e17ada5a5db23f4db867",
        size_bytes: 63_786_048,
        recommended: false,
    },
    CatalogEntry {
        id: DEFAULT_LOCAL_MODEL_ID,
        name: "Whisper Small",
        description: "Recommended. Noticeably more accurate, still quick on a modern CPU.",
        family: "Whisper",
        parameters: "242M",
        language_count: 99,
        accuracy: 80,
        speed: 78,
        streaming: false,
        repo: "handy-computer/whisper-small-gguf",
        revision: "a2073177cb69bd74b9ca9460b852d17fbfd5d68c",
        filename: "whisper-small-Q5_K_M.gguf",
        sha256: "326cd00c3e7217c751667c7c1600eaf7e0de174e186ca2c16b4bf590251c3c3b",
        size_bytes: 193_749_056,
        recommended: true,
    },
    CatalogEntry {
        id: "whisper-medium",
        name: "Whisper Medium",
        description: "Higher accuracy for accents and noisy rooms. Slower on CPU.",
        family: "Whisper",
        parameters: "764M",
        language_count: 99,
        accuracy: 84,
        speed: 42,
        streaming: false,
        repo: "handy-computer/whisper-medium-gguf",
        revision: "835ad19fff976143d650e35113a6544980cbd983",
        filename: "whisper-medium-Q4_K_M.gguf",
        sha256: "6c9cad9c41d1fa0fcc624898f7ee3485b66127627878081a5655c6f6f3ce20ab",
        size_bytes: 504_102_848,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo",
        description: "Best quality for its size, with broad language coverage.",
        family: "Whisper",
        parameters: "809M",
        language_count: 100,
        accuracy: 88,
        speed: 35,
        streaming: false,
        repo: "handy-computer/whisper-large-v3-turbo-gguf",
        revision: "ceea6c8a94a21ab85be244d311e874a39344dbf5",
        filename: "whisper-large-v3-turbo-Q4_K_M.gguf",
        sha256: "ecfe9b6beb4ab18fef49187cc968cc74b5168b94629c8830e2ca6b794c6e25ed",
        size_bytes: 536_069_728,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-large-v3",
        name: "Whisper Large v3",
        description: "Highest Whisper accuracy and the widest language coverage. Needs a capable machine.",
        family: "Whisper",
        parameters: "1.5B",
        language_count: 100,
        accuracy: 89,
        speed: 23,
        streaming: false,
        repo: "handy-computer/whisper-large-v3-gguf",
        revision: "e3e29bee6389c7da4a141406f07bb80ddac5337c",
        filename: "whisper-large-v3-Q5_K_M.gguf",
        sha256: "6053d0fd69a0fd48b8fea5ea7a52b9e0cde389343566fa30e453a1b2b258dc38",
        size_bytes: 1_161_143_008,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-base-en",
        name: "Whisper Base (English)",
        description: "English-only Base. A little more accurate than multilingual Base.",
        family: "Whisper",
        parameters: "73M",
        language_count: 1,
        accuracy: 76,
        speed: 99,
        streaming: false,
        repo: "handy-computer/whisper-base.en-gguf",
        revision: "cf0804db15fb341d00c9274b90da9cbb4fe2e5c6",
        filename: "whisper-base.en-Q8_0.gguf",
        sha256: "3b46ca40bccbf7609c68d88a36d96077a04ca7c87f2060ede06f129fac3e7652",
        size_bytes: 84_886_208,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-small-en",
        name: "Whisper Small (English)",
        description: "English-only Small. Accurate and quick for English dictation.",
        family: "Whisper",
        parameters: "242M",
        language_count: 1,
        accuracy: 81,
        speed: 80,
        streaming: false,
        repo: "handy-computer/whisper-small.en-gguf",
        revision: "41b0f75fd44415ba127a5356c5ba9ed450c1debd",
        filename: "whisper-small.en-Q8_0.gguf",
        sha256: "9614e6b7fda2d26018e4f268aece8ca25a83296ea0b534169a585b740bfd71ef",
        size_bytes: 269_674_144,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-medium-en",
        name: "Whisper Medium (English)",
        description: "English-only Medium. Strong accuracy for English, slower on CPU.",
        family: "Whisper",
        parameters: "764M",
        language_count: 1,
        accuracy: 83,
        speed: 44,
        streaming: false,
        repo: "handy-computer/whisper-medium.en-gguf",
        revision: "f25c70d9095dcfdad187ebb3b113d157b414aee8",
        filename: "whisper-medium.en-Q8_0.gguf",
        sha256: "03d7257fef498750ce272631bc6a34de322fc2b438aab5c268ff49dfd1b64c49",
        size_bytes: 831_460_928,
        recommended: false,
    },
    CatalogEntry {
        id: "parakeet-tdt-0.6b-v3",
        name: "Parakeet TDT 0.6B v3",
        description: "Fast and accurate across 25 European languages.",
        family: "Parakeet",
        parameters: "0.6B",
        language_count: 25,
        accuracy: 88,
        speed: 79,
        streaming: false,
        repo: "handy-computer/parakeet-tdt-0.6b-v3-gguf",
        revision: "85ac09ea12fc4b1112fa76810059364bc6adc9de",
        filename: "parakeet-tdt-0.6b-v3-Q8_0.gguf",
        sha256: "5859f77944efcd8eafa23a6350731960b2b55b2203df51f319665c807d802cc7",
        size_bytes: 739_508_576,
        recommended: false,
    },
    CatalogEntry {
        id: "parakeet-tdt-0.6b-v2",
        name: "Parakeet TDT 0.6B v2",
        description: "English only. NVIDIA's best English model for clear speech.",
        family: "Parakeet",
        parameters: "0.6B",
        language_count: 1,
        accuracy: 89,
        speed: 85,
        streaming: false,
        repo: "handy-computer/parakeet-tdt-0.6b-v2-gguf",
        revision: "07cee0616125a08ef619729bb47f40ef747e4bc4",
        filename: "parakeet-tdt-0.6b-v2-Q8_0.gguf",
        sha256: "f0d0e99cebb6d3b83f1f7069b82b5d3c2e39a54545b0da039cb4bafd9c4e5caa",
        size_bytes: 729_574_912,
        recommended: false,
    },
    CatalogEntry {
        id: "parakeet-tdt-ctc-110m",
        name: "Parakeet TDT-CTC 110M",
        description: "Very fast and tiny. Great for quick English notes on any machine.",
        family: "Parakeet",
        parameters: "110M",
        language_count: 1,
        accuracy: 85,
        speed: 98,
        streaming: false,
        repo: "handy-computer/parakeet-tdt_ctc-110m-gguf",
        revision: "9d66d34f9e1594075c5dd72c90c0f4c321b29f21",
        filename: "parakeet-tdt_ctc-110m-Q8_0.gguf",
        sha256: "7dd44c74a331d788a4e5f8b16913b3feb29ced22cf5613aad0e0f6cd30516296",
        size_bytes: 135_373_280,
        recommended: false,
    },
    CatalogEntry {
        id: "parakeet-unified-en-0.6b",
        name: "Parakeet Unified EN 0.6B",
        description: "English streaming model with excellent accuracy.",
        family: "Parakeet",
        parameters: "0.6B",
        language_count: 1,
        accuracy: 90,
        speed: 79,
        streaming: true,
        repo: "handy-computer/parakeet-unified-en-0.6b-gguf",
        revision: "7e948f21b7bdbac698d3318db9d350f1096f3b6c",
        filename: "parakeet-unified-en-0.6b-Q8_0.gguf",
        sha256: "4b50b6dd862bf6e346929aaf4f5eaacec003bfa3f56462d6c874b41ef2f38795",
        size_bytes: 731_357_568,
        recommended: false,
    },
    CatalogEntry {
        id: "parakeet-tdt-1.1b",
        name: "Parakeet TDT 1.1B",
        description: "English only. Highest Parakeet accuracy, heavier on CPU.",
        family: "Parakeet",
        parameters: "1.1B",
        language_count: 1,
        accuracy: 91,
        speed: 76,
        streaming: false,
        repo: "handy-computer/parakeet-tdt-1.1b-gguf",
        revision: "8c21810615694c53a4f4745996190fcca880f8e5",
        filename: "parakeet-tdt-1.1b-Q5_K_M.gguf",
        sha256: "18c22888fed10676fde72fdd5b833fab9175a6abf4e0cb390eb0a59b0b107cd3",
        size_bytes: 935_758_496,
        recommended: false,
    },
    CatalogEntry {
        id: "canary-180m-flash",
        name: "Canary 180M Flash",
        description: "Tiny and instant across four languages. Runs on any hardware.",
        family: "Canary",
        parameters: "180M",
        language_count: 4,
        accuracy: 88,
        speed: 98,
        streaming: false,
        repo: "handy-computer/canary-180m-flash-gguf",
        revision: "b147f9dc52b59f0998e410540a84727bd86457fd",
        filename: "canary-180m-flash-Q8_0.gguf",
        sha256: "e13c7f5d0952b056a027cfffec13e3a3a134d1608babed24f983568f141e297c",
        size_bytes: 218_447_552,
        recommended: false,
    },
    CatalogEntry {
        id: "canary-1b-v2",
        name: "Canary 1B v2",
        description: "25-language speech-to-text with translation support.",
        family: "Canary",
        parameters: "1B",
        language_count: 25,
        accuracy: 88,
        speed: 81,
        streaming: false,
        repo: "handy-computer/canary-1b-v2-gguf",
        revision: "58d13c2c0102229aad45f7e19a77ddc42b41dd9a",
        filename: "canary-1b-v2-Q5_K_M.gguf",
        sha256: "9c3a893c93795438baf9b4b1c853c39b60316c3a0d259a3ba6e284712f5ddb71",
        size_bytes: 836_664_032,
        recommended: false,
    },
    CatalogEntry {
        id: "moonshine-tiny",
        name: "Moonshine Tiny",
        description: "Very small English model. Fast, for short commands and notes.",
        family: "Moonshine",
        parameters: "27M",
        language_count: 1,
        accuracy: 74,
        speed: 100,
        streaming: false,
        repo: "handy-computer/moonshine-tiny-gguf",
        revision: "f5c11906eba3f44cf305eed30feb9cbfb0b4b9d0",
        filename: "moonshine-tiny-Q8_0.gguf",
        sha256: "2fd348d7b38f97d309cc3ec6848f3f57f537b80244950f07d2637e463f95a3a1",
        size_bytes: 35_466_912,
        recommended: false,
    },
    CatalogEntry {
        id: "moonshine-base",
        name: "Moonshine Base",
        description: "Small English model balancing speed and accuracy.",
        family: "Moonshine",
        parameters: "62M",
        language_count: 1,
        accuracy: 80,
        speed: 99,
        streaming: false,
        repo: "handy-computer/moonshine-base-gguf",
        revision: "3ef112378a8cf46ac8b278d9bfa2d15c846704b8",
        filename: "moonshine-base-Q8_0.gguf",
        sha256: "7f0027dfd857d310b63a85ef57cadf183da712cc374f85a648f8bc18aaa2efc8",
        size_bytes: 77_476_480,
        recommended: false,
    },
    CatalogEntry {
        id: "moonshine-streaming-small",
        name: "Moonshine Streaming Small",
        description: "English streaming model with a good speed/accuracy balance.",
        family: "Moonshine",
        parameters: "140M",
        language_count: 1,
        accuracy: 84,
        speed: 95,
        streaming: true,
        repo: "handy-computer/moonshine-streaming-small-gguf",
        revision: "41444173ed8210852a883e046fadcfba3e7bfbae",
        filename: "moonshine-streaming-small-Q8_0.gguf",
        sha256: "d03670f69629b649085d0f44a63d97668b4119117cc9611a4e4ad94341713dfc",
        size_bytes: 198_506_848,
        recommended: false,
    },
    CatalogEntry {
        id: "moonshine-streaming-medium",
        name: "Moonshine Streaming Medium",
        description: "English streaming model, more accurate than Small.",
        family: "Moonshine",
        parameters: "266M",
        language_count: 1,
        accuracy: 87,
        speed: 83,
        streaming: true,
        repo: "handy-computer/moonshine-streaming-medium-gguf",
        revision: "c722a9455a40a1844c3d25267dc84eff61d8dd84",
        filename: "moonshine-streaming-medium-Q8_0.gguf",
        sha256: "f7c9564249b508f6012927ec4f9e536087da53a7047f858ca9975bea5f75299e",
        size_bytes: 295_793_568,
        recommended: false,
    },
    CatalogEntry {
        id: "sensevoice-small",
        name: "SenseVoice Small",
        description: "Compact multilingual model strong on Chinese, Cantonese, English, Japanese, and Korean.",
        family: "SenseVoice",
        parameters: "234M",
        language_count: 5,
        accuracy: 81,
        speed: 98,
        streaming: false,
        repo: "handy-computer/SenseVoiceSmall-gguf",
        revision: "4a08b8e900b38a977e32eb08d5d0697d6e72ba04",
        filename: "SenseVoiceSmall-Q8_0.gguf",
        sha256: "6c759ee4c9748c9b3f7a5a60ca74f0f7e685fb9d45d1378fce7cfd62f59adf29",
        size_bytes: 252_684_608,
        recommended: false,
    },
    CatalogEntry {
        id: "qwen3-asr-0.6b",
        name: "Qwen3-ASR 0.6B",
        description: "Compact model covering 30 languages, including Chinese and Cantonese.",
        family: "Qwen3-ASR",
        parameters: "782M",
        language_count: 30,
        accuracy: 87,
        speed: 63,
        streaming: false,
        repo: "handy-computer/Qwen3-ASR-0.6B-gguf",
        revision: "e4e16599b900eb0cb36e524514756bb92eb092b7",
        filename: "Qwen3-ASR-0.6B-Q8_0.gguf",
        sha256: "f081b2d5e23bd669d92cc331d722a8a0681943b8e6f34b48996fd5c319b5acd8",
        size_bytes: 850_423_456,
        recommended: false,
    },
    CatalogEntry {
        id: "qwen3-asr-1.7b",
        name: "Qwen3-ASR 1.7B",
        description: "Larger 30-language model with higher accuracy than 0.6B.",
        family: "Qwen3-ASR",
        parameters: "2.0B",
        language_count: 30,
        accuracy: 90,
        speed: 38,
        streaming: false,
        repo: "handy-computer/Qwen3-ASR-1.7B-gguf",
        revision: "92282af1610a2db19d66f2bef1e260f5deca782d",
        filename: "Qwen3-ASR-1.7B-Q5_K_M.gguf",
        sha256: "034c557fe92ff8fcd9a9c041cbdaad347be0a86a58d3a348f63cf3f0180879d0",
        size_bytes: 1_517_290_464,
        recommended: false,
    },
    CatalogEntry {
        id: "cohere-transcribe-03-2026",
        name: "Cohere Transcribe",
        description: "Highest accuracy on this list, across 14 languages. Slower on CPU.",
        family: "Cohere",
        parameters: "2.0B",
        language_count: 14,
        accuracy: 92,
        speed: 63,
        streaming: false,
        repo: "handy-computer/cohere-transcribe-03-2026-gguf",
        revision: "dfa4adebb64f3076b7b6b90b721275cc069cb421",
        filename: "cohere-transcribe-03-2026-Q5_K_M.gguf",
        sha256: "14d02f1ad6dd77b3a60f82639879012c3adb4fe25c50a5a47a2c4c661daf1558",
        size_bytes: 1_770_270_208,
        recommended: false,
    },
    CatalogEntry {
        id: "nemotron-3.5-asr-streaming-0.6b",
        name: "Nemotron Streaming 3.5",
        description: "Live multilingual transcription across 28 languages.",
        family: "Nemotron",
        parameters: "0.6B",
        language_count: 28,
        accuracy: 82,
        speed: 84,
        streaming: true,
        repo: "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf",
        revision: "6d44e540bc31b0de1dbe174a3cea87f53a7f22fb",
        filename: "nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf",
        sha256: "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c",
        size_bytes: 751_094_240,
        recommended: false,
    },
    CatalogEntry {
        id: "granite-speech-4.1-2b-nar",
        name: "Granite Speech 4.1 2B NAR",
        description: "High-accuracy five-language model. Heavier and slower on CPU.",
        family: "Granite",
        parameters: "2.3B",
        language_count: 5,
        accuracy: 92,
        speed: 37,
        streaming: false,
        repo: "handy-computer/granite-speech-4.1-2b-nar-gguf",
        revision: "ca53e8273416eb7e888f19bcebbcb9b6ab3edc17",
        filename: "granite-speech-4.1-2b-nar-Q5_K_M.gguf",
        sha256: "88d7c7b5b8b59c95bb6580a1e7d5d81cae63943cef71405ff477527b0bb69fca",
        size_bytes: 1_782_089_344,
        recommended: false,
    },
    CatalogEntry {
        id: "voxtral-mini-4b-realtime-2602",
        name: "Voxtral Mini 4B Realtime",
        description: "Streaming 13-language model. Large, and best on a GPU.",
        family: "Voxtral",
        parameters: "4.8B",
        language_count: 13,
        accuracy: 87,
        speed: 11,
        streaming: true,
        repo: "handy-computer/Voxtral-Mini-4B-Realtime-2602-gguf",
        revision: "b3e1c979e3775cbd0a49a65878a0ec7f06789ed7",
        filename: "Voxtral-Mini-4B-Realtime-2602-Q5_K_M.gguf",
        sha256: "e20a7582c5cf8159454c909b7f57b184b287cb3d7c3ec85744727a2cec51b08f",
        size_bytes: 3_281_439_008,
        recommended: false,
    },
    CatalogEntry {
        id: "gigaam-v3-e2e-rnnt",
        name: "GigaAM v3 E2E-RNN-T",
        description: "Russian speech-to-text with token-level timestamps.",
        family: "GigaAM",
        parameters: "223M",
        language_count: 1,
        accuracy: 70,
        speed: 94,
        streaming: false,
        repo: "handy-computer/gigaam-v3-e2e-rnnt-gguf",
        revision: "f719d70812344f4d0fb8c11c0887b190501a7465",
        filename: "gigaam-v3-e2e-rnnt-Q8_0.gguf",
        sha256: "78d63b47723b7f8d78c6113a6ef983b5a86e2a86f6c273e1f5cb6967b1c4467a",
        size_bytes: 273_724_832,
        recommended: false,
    },
    CatalogEntry {
        id: "fun-asr-mlt-nano-2512",
        name: "Fun-ASR Nano Multilingual",
        description: "Compact 31-language model with strong Asian language coverage.",
        family: "Fun-ASR",
        parameters: "985M",
        language_count: 31,
        accuracy: 89,
        speed: 68,
        streaming: false,
        repo: "handy-computer/Fun-ASR-MLT-Nano-2512-gguf",
        revision: "0b8f9c7bc545a219658aeb1dd4eeaa55d1cf89f3",
        filename: "Fun-ASR-MLT-Nano-2512-Q8_0.gguf",
        sha256: "d12476d8d9f2baa0ebf738fa955fa05ed33a654f1567289033a810c45d9d9002",
        size_bytes: 891_271_232,
        recommended: false,
    },
];

/// A model as the UI sees it (no URL, hash, or filesystem path).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSpeechModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub recommended: bool,
    pub downloaded: bool,
    /// 0–100 quality score (higher is more accurate), for the comparison bars.
    pub accuracy: u8,
    /// 0–100 speed score (higher is faster), for the comparison bars.
    pub speed: u8,
    pub family: String,
    pub parameters: String,
    pub language_count: u16,
    pub streaming: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug)]
pub enum ModelStoreError {
    UnknownModel,
    Unavailable,
    Download,
    /// SHA-256 mismatch: the file was removed and the download can be retried.
    Corrupt,
    Io(io::Error),
}

impl fmt::Display for ModelStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownModel => "That speech model is not available.",
            Self::Unavailable => "The speech models folder is unavailable.",
            Self::Download => {
                "The model could not be downloaded. Check your connection and try again."
            }
            Self::Corrupt => {
                "The downloaded model failed its integrity check and was removed. Try again."
            }
            Self::Io(_) => "The speech models folder could not be read or written.",
        })
    }
}

impl std::error::Error for ModelStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ModelStoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

fn catalog_entry(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

pub struct ModelStore {
    directory: PathBuf,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl ModelStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            cancels: Mutex::new(HashMap::new()),
        }
    }

    /// Resolved path for a downloaded model, if the file is present.
    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        let entry = catalog_entry(id)?;
        let path = self.directory.join(entry.filename);
        path.is_file().then_some(path)
    }

    pub fn is_downloaded(&self, id: &str) -> bool {
        self.path_for(id).is_some()
    }

    /// `id` of the first installed model in catalog order, if any.
    pub fn first_downloaded(&self) -> Option<&'static str> {
        CATALOG
            .iter()
            .find(|entry| self.is_downloaded(entry.id))
            .map(|entry| entry.id)
    }

    pub fn list(&self) -> Vec<LocalSpeechModel> {
        CATALOG
            .iter()
            .map(|entry| LocalSpeechModel {
                id: entry.id.to_owned(),
                name: entry.name.to_owned(),
                description: entry.description.to_owned(),
                size_bytes: entry.size_bytes,
                recommended: entry.recommended,
                downloaded: self.is_downloaded(entry.id),
                accuracy: entry.accuracy,
                speed: entry.speed,
                family: entry.family.to_owned(),
                parameters: entry.parameters.to_owned(),
                language_count: entry.language_count,
                streaming: entry.streaming,
            })
            .collect()
    }

    pub async fn download(&self, app: &AppHandle, id: &str) -> Result<(), ModelStoreError> {
        let entry = catalog_entry(id).ok_or(ModelStoreError::UnknownModel)?;
        let target = self.directory.join(entry.filename);
        if target.is_file() {
            return Ok(());
        }
        tokio::fs::create_dir_all(&self.directory)
            .await
            .map_err(|_| ModelStoreError::Unavailable)?;
        // A leftover `.partial` is resumed rather than discarded: dropping the
        // connection halfway through a 500 MB model should not restart it.
        // The final SHA-256 check still rejects a partial that came from a
        // different file.
        let partial = self.directory.join(format!("{}.partial", entry.filename));

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels
            .lock()
            .map_err(|_| ModelStoreError::Unavailable)?
            .insert(id.to_owned(), Arc::clone(&cancel));
        let result = self.stream_to_file(app, entry, &partial, &cancel).await;
        self.cancels.lock().ok().map(|mut map| map.remove(id));
        if let Err(error) = result {
            // Keep an interrupted download for the next attempt; a cancel is
            // deliberate, so discard it.
            if cancel.load(Ordering::SeqCst) {
                let _ = tokio::fs::remove_file(&partial).await;
            }
            return Err(error);
        }

        // Verify off the async runtime: hashing a few hundred MB is CPU work.
        let expected = entry.sha256.to_owned();
        let verify_path = partial.clone();
        let valid = tokio::task::spawn_blocking(move || verify_sha256(&verify_path, &expected))
            .await
            .map_err(|_| ModelStoreError::Unavailable)?
            .unwrap_or(false);
        if !valid {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(ModelStoreError::Corrupt);
        }
        tokio::fs::rename(&partial, &target)
            .await
            .map_err(|_| ModelStoreError::Unavailable)?;
        let _ = app.emit("local-models-changed", self.list());
        Ok(())
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(map) = self.cancels.lock()
            && let Some(flag) = map.get(id)
        {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Deletes an installed model. The file is removed even if it is currently
    /// selected; the next dictation reports the model as missing.
    pub fn delete(&self, id: &str) -> Result<(), ModelStoreError> {
        let entry = catalog_entry(id).ok_or(ModelStoreError::UnknownModel)?;
        let path = self.directory.join(entry.filename);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn stream_to_file(
        &self,
        app: &AppHandle,
        entry: &CatalogEntry,
        partial: &Path,
        cancel: &AtomicBool,
    ) -> Result<(), ModelStoreError> {
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            entry.repo, entry.revision, entry.filename
        );
        let existing = tokio::fs::metadata(partial)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| ModelStoreError::Download)?;
        let mut request = client.get(&url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|_| ModelStoreError::Download)?;
        let plan = plan_download(
            existing,
            response.status().as_u16(),
            response.content_length(),
            entry.size_bytes,
        )
        .map_err(|_| ModelStoreError::Download)?;

        let (mut file, mut downloaded, total) = match plan {
            // A previous attempt wrote the whole file but never verified it;
            // the hash check below still decides.
            DownloadPlan::Complete => return Ok(()),
            DownloadPlan::Resume { offset, total } => {
                let file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(partial)
                    .await
                    .map_err(|_| ModelStoreError::Unavailable)?;
                (file, offset, total)
            }
            // A server that ignores Range restarts from zero, so truncate.
            DownloadPlan::Restart { total } => {
                let file = tokio::fs::File::create(partial)
                    .await
                    .map_err(|_| ModelStoreError::Unavailable)?;
                (file, 0, total)
            }
        };
        let mut stream = response.bytes_stream();
        let mut last_emit = Instant::now();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                return Err(ModelStoreError::Download);
            }
            let chunk = chunk.map_err(|_| ModelStoreError::Download)?;
            file.write_all(&chunk)
                .await
                .map_err(|_| ModelStoreError::Unavailable)?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                last_emit = Instant::now();
                let _ = app.emit(
                    "local-model-progress",
                    LocalModelProgress {
                        model_id: entry.id.to_owned(),
                        downloaded,
                        total,
                    },
                );
            }
        }
        file.flush()
            .await
            .map_err(|_| ModelStoreError::Unavailable)?;
        drop(file);
        let _ = app.emit(
            "local-model-progress",
            LocalModelProgress {
                model_id: entry.id.to_owned(),
                downloaded,
                total,
            },
        );
        Ok(())
    }
}

/// How a download response maps onto the `.partial` file. `Err(())` is a
/// non-success status the caller turns into a retryable download error.
#[derive(Debug, PartialEq, Eq)]
enum DownloadPlan {
    /// 416: the partial already holds every byte the server has.
    Complete,
    /// 206: append the remaining bytes after `offset`.
    Resume { offset: u64, total: u64 },
    /// 200 (or a server that ignored `Range`): truncate and start over.
    Restart { total: u64 },
}

fn plan_download(
    existing: u64,
    status: u16,
    content_length: Option<u64>,
    fallback_size: u64,
) -> Result<DownloadPlan, ()> {
    if status == 416 {
        return Ok(DownloadPlan::Complete);
    }
    let resumed = status == 206;
    if !resumed && !(200..300).contains(&status) {
        return Err(());
    }
    if resumed {
        Ok(DownloadPlan::Resume {
            offset: existing,
            total: content_length
                .map(|remaining| remaining + existing)
                .unwrap_or(fallback_size),
        })
    } else {
        Ok(DownloadPlan::Restart {
            total: content_length.unwrap_or(fallback_size),
        })
    }
}

fn verify_sha256(path: &Path, expected: &str) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    // sha2 0.11 dropped the io::Write impl (digest 0.11), so stream the file
    // through update() in fixed-size chunks instead of io::copy.
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::{DownloadPlan, ModelStore, catalog_entry, plan_download, verify_sha256};

    #[test]
    fn download_plan_resumes_and_restarts_correctly() {
        // 206 appends after the bytes already on disk and reports the real total.
        assert_eq!(
            plan_download(100, 206, Some(900), 1000),
            Ok(DownloadPlan::Resume {
                offset: 100,
                total: 1000
            })
        );
        // 206 without a length falls back to the catalog size.
        assert_eq!(
            plan_download(100, 206, None, 1000),
            Ok(DownloadPlan::Resume {
                offset: 100,
                total: 1000
            })
        );
        // 200 means the server ignored Range: truncate and start over.
        assert_eq!(
            plan_download(100, 200, Some(1000), 1000),
            Ok(DownloadPlan::Restart { total: 1000 })
        );
        // 416 means the partial already covers the file; let verification decide.
        assert_eq!(
            plan_download(1000, 416, None, 1000),
            Ok(DownloadPlan::Complete)
        );
        // Anything else is a retryable failure.
        assert!(plan_download(0, 500, None, 1000).is_err());
        assert!(plan_download(0, 404, None, 1000).is_err());
    }

    #[test]
    fn every_catalog_entry_is_well_formed() {
        let mut ids = std::collections::HashSet::new();
        let mut filenames = std::collections::HashSet::new();
        for entry in super::CATALOG {
            assert!(!entry.id.is_empty());
            assert!(ids.insert(entry.id), "duplicate model id: {}", entry.id);
            assert!(
                filenames.insert(entry.filename),
                "duplicate filename: {}",
                entry.filename
            );
            assert!(!entry.name.is_empty());
            assert!(!entry.description.is_empty());
            assert!(!entry.family.is_empty());
            assert!(entry.sha256.len() == 64);
            assert!(entry.revision.len() >= 7);
            assert!(entry.size_bytes > 0);
            assert!(entry.accuracy <= 100);
            assert!(entry.speed <= 100);
            assert!(entry.language_count >= 1);
        }
        let default = catalog_entry(super::DEFAULT_LOCAL_MODEL_ID).unwrap();
        assert!(default.recommended);
        assert_eq!(default.family, "Whisper");
    }

    #[test]
    fn sha256_matches_known_digest() {
        // "abc" -> ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let path = std::env::temp_dir().join("kivo-sha-test.bin");
        std::fs::write(&path, b"abc").unwrap();
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(&path, expected).unwrap());
        assert!(!verify_sha256(&path, &"0".repeat(64)).unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_and_unknown_models_report_not_downloaded() {
        let store = ModelStore::new(std::env::temp_dir().join("kivo-absent-models"));
        assert!(!store.is_downloaded("whisper-small"));
        assert!(store.path_for("does-not-exist").is_none());
        assert!(store.first_downloaded().is_none());
    }
}
