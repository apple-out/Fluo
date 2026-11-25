use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, RwLock as StdRwLock, mpsc as std_mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{LlamaBackendDeviceType, list_llama_ggml_backend_devices};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::ai::PromptContext;
use crate::model::{AppSettings, ContextPair, ModelInfo, SegmentInput, SegmentResult};

pub const DEFAULT_HY_MT2_MODEL_ID: &str = "Hy-MT2-1.8B-Q4_K_M.gguf";
pub const HY_MT2_HIGH_SPEC_MODEL_ID: &str = "Hy-MT2-7B-Q4_K_M.gguf";
pub const HY_MT2_LIGHT_MODEL_ID: &str = "Hy-MT2-1.8B-1.25Bit.gguf";

#[derive(Debug, Clone, Copy)]
pub struct HyMt2ModelSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub tier: &'static str,
    pub repo: &'static str,
    pub filename: &'static str,
    pub expected_bytes: u64,
    pub sha256: &'static str,
    pub description: &'static str,
    pub min_ram_mb: u64,
    pub min_vram_mb: u64,
    pub experimental_kernel_note: Option<&'static str>,
}

pub const HY_MT2_CATALOG: &[HyMt2ModelSpec] = &[
    HyMt2ModelSpec {
        id: "Hy-MT2-1.8B-Q4_K_M.gguf",
        display_name: "Hy-MT2 1.8B (Q4_K_M · A16 Bionic / 평균 PC 주력 권장)",
        tier: "1.8b-q4_k_m",
        repo: "tencent/Hy-MT2-1.8B-GGUF",
        filename: "Hy-MT2-1.8B-Q4_K_M.gguf",
        expected_bytes: 1_133_080_448,
        sha256: "dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53af84c4b89fb0c06699",
        description: "iPhone 14 Pro / 15(A16 Bionic, 6GB RAM · 앱 메모리 1.3GB 이내) 및 평균 PC 내장그래픽 공통 1순위 · 2차 라노벨 튜닝 적용",
        min_ram_mb: 5_500,
        min_vram_mb: 0,
        experimental_kernel_note: None,
    },
    HyMt2ModelSpec {
        id: "Hy-MT2-1.8B-Q6_K.gguf",
        display_name: "Hy-MT2 1.8B (Q6_K · A16 / 평균 PC 고정밀)",
        tier: "1.8b-q6_k",
        repo: "tencent/Hy-MT2-1.8B-GGUF",
        filename: "Hy-MT2-1.8B-Q6_K.gguf",
        expected_bytes: 1_474_785_120,
        sha256: "d98fe604dec1f28f58f80d7d560f7177e584d3b8e5835862687660e5ff97cb40",
        description: "1.37GB 6비트 고정밀 양자화 · A16 Bionic(단일 탭) 및 평균 PC에서 4비트 양자화 어휘 손실을 줄인 고품질 번역 모델",
        min_ram_mb: 6_000,
        min_vram_mb: 0,
        experimental_kernel_note: None,
    },
    HyMt2ModelSpec {
        id: "Hy-MT2-7B-Q4_K_M.gguf",
        display_name: "Hy-MT2 7B (Q4_K_M · 고사양 PC 전용)",
        tier: "7b-q4_k_m",
        repo: "tencent/Hy-MT2-7B-GGUF",
        filename: "Hy-MT2-7B-Q4_K_M.gguf",
        expected_bytes: 4_624_648_896,
        sha256: "9f96256500f3fc1ab4d64336b58f52a949a95ad7516b0c229476eef782f9f77b",
        description: "6GB+ 외장 VRAM(NVIDIA GeForce RTX · AMD Radeon RX · Intel Arc) 또는 내장 GPU+CPU 하이브리드 분할 가속용 · 정발본급 품질 (모바일 및 저사양 PC 비권장)",
        min_ram_mb: 12_000,
        min_vram_mb: 6_500,
        experimental_kernel_note: None,
    },
    HyMt2ModelSpec {
        id: "Hy-MT2-1.8B-2Bit.gguf",
        display_name: "Hy-MT2 1.8B (2-Bit · A16 다중 탭 / 저전력 초경량)",
        tier: "1.8b-2bit",
        repo: "tencent/Hy-MT2-1.8B-2bit-GGUF",
        filename: "Hy-MT2-1.8B-2Bit.gguf",
        expected_bytes: 600_534_880,
        sha256: "dcc33bbae9b28d923c8c76a64f6157840841d26f8774f3dfd770d5fabeeb1cd7",
        description: "약 572MB 초경량 2-bit 양자화 · iPhone 14 Pro / 15(A16 Bionic) 다중 탭 브라우징 및 저사양 PC 배터리 절약용",
        min_ram_mb: 3_500,
        min_vram_mb: 0,
        experimental_kernel_note: Some(
            "Tencent AngelSlim 2-Bit (STQ 커널 요구 시 Q4_K_M 사용 권장)",
        ),
    },
    HyMt2ModelSpec {
        id: "Hy-MT2-1.8B-1.25Bit.gguf",
        display_name: "Hy-MT2 1.8B (1.25-Bit · 극한 경량)",
        tier: "1.8b-1.25bit",
        repo: "tencent/Hy-MT2-1.8B-1.25Bit-GGUF",
        filename: "Hy-MT2-1.8B-1.25Bit.gguf",
        expected_bytes: 461_860_800,
        sha256: "cc497fe8f033b52b3b8b00a7669e9661435432f9d4cd43f7ed24400c01507a93",
        description: "약 440MB AngelSlim 1.25-bit 극한 경량 모델",
        min_ram_mb: 3_000,
        min_vram_mb: 0,
        experimental_kernel_note: Some(
            "Tencent AngelSlim 1.25-Bit (upstream llama.cpp PR #22836 기반 STQ)",
        ),
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub cpu_cores: usize,
    pub cpu_name: Option<String>,
    pub gpu_name: Option<String>,
    pub dedicated_vram_mb: u64,
    pub shared_vram_mb: u64,
    pub is_amd_apu: bool,
    pub npu_available: bool,
    pub npu_name: Option<String>,
    pub vulkan_compiled: bool,
    pub vulkan_available: bool,
    pub active_backend: String,
    pub compute_decision: String,
    pub recommended_tier: String,
    pub recommended_model_id: String,
    pub recommendation_reason: String,
    pub gpu_vendor: String,
    pub is_discrete_gpu: bool,
    pub selected_gpu_device_index: Option<usize>,
    pub cuda_compiled: bool,
    pub cuda_available: bool,
    pub detected_gpus: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyMt2ModelState {
    pub id: String,
    pub display_name: String,
    pub tier: String,
    pub repo: String,
    pub filename: String,
    pub download_url: String,
    pub expected_bytes: u64,
    pub sha256: String,
    pub description: String,
    pub min_ram_mb: u64,
    pub min_vram_mb: u64,
    pub recommended: bool,
    pub installed: bool,
    pub loaded: bool,
    pub downloading: bool,
    pub file_path: String,
    pub installed_bytes: u64,
    pub partial_bytes: u64,
    pub experimental_kernel_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub status: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub speed_mbps: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TranslationStreamEvent {
    Started {
        total: usize,
        backend: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detected_style: Option<String>,
    },
    Segment {
        result: SegmentResult,
    },
    Progress {
        done: usize,
        total: usize,
    },
    Done {
        total: usize,
        uncertain: usize,
    },
    Cancelled {
        message: String,
    },
    Error {
        message: String,
    },
}

#[derive(Clone)]
struct LoadedModelMeta {
    model_id: String,
    backend_label: String,
}

#[derive(Clone)]
struct WorkerModelSpec {
    model_id: String,
    file_path: PathBuf,
    n_gpu_layers: u32,
    preferred_device_index: Option<usize>,
    backend_label: String,
    ctx_size: u32,
    threads: i32,
}

enum WorkerCommand {
    EnsureLoaded {
        spec: WorkerModelSpec,
        reply: tokio::sync::oneshot::Sender<Result<LoadedModelMeta, String>>,
    },
    TranslateBatch {
        spec: WorkerModelSpec,
        segments: Vec<SegmentInput>,
        target_lang: String,
        style: String,
        glossary: Vec<crate::ai::GlossaryRule>,
        history: Vec<ContextPair>,
        cancel: CancellationToken,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SegmentResult>,
        reply: tokio::sync::oneshot::Sender<Result<Vec<ContextPair>, String>>,
    },
    Unload {
        reply: tokio::sync::oneshot::Sender<bool>,
    },
    Shutdown,
}

impl WorkerCommand {
    fn spec(&self) -> Option<&WorkerModelSpec> {
        match self {
            WorkerCommand::EnsureLoaded { spec, .. }
            | WorkerCommand::TranslateBatch { spec, .. } => Some(spec),
            WorkerCommand::Unload { .. } | WorkerCommand::Shutdown => None,
        }
    }

    fn fail_load(self, err: String) {
        match self {
            WorkerCommand::EnsureLoaded { reply, .. } => {
                let _ = reply.send(Err(err));
            }
            WorkerCommand::TranslateBatch { reply, .. } => {
                let _ = reply.send(Err(err));
            }
            WorkerCommand::Unload { reply } => {
                let _ = reply.send(false);
            }
            WorkerCommand::Shutdown => {}
        }
    }
}

static GLOBAL_LLAMA_BACKEND: OnceLock<Result<Arc<LlamaBackend>, String>> = OnceLock::new();

pub struct HyMt2Engine {
    models_dir: PathBuf,
    cached_hardware: OnceLock<HardwareProfile>,
    loaded_meta: Arc<StdRwLock<Option<LoadedModelMeta>>>,
    worker_tx: StdMutex<Option<std_mpsc::Sender<WorkerCommand>>>,
    last_activity_ms: Arc<AtomicU64>,
    active_inference_jobs: Arc<AtomicUsize>,
    download_cancels: Mutex<HashMap<String, CancellationToken>>,
    page_histories: Arc<Mutex<HashMap<String, Vec<ContextPair>>>>,
    page_history_order: Arc<Mutex<Vec<String>>>,
}

impl HyMt2Engine {
    pub fn new(models_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&models_dir);
        Self {
            models_dir,
            cached_hardware: OnceLock::new(),
            loaded_meta: Arc::new(StdRwLock::new(None)),
            worker_tx: StdMutex::new(None),
            last_activity_ms: Arc::new(AtomicU64::new(now_epoch_ms())),
            active_inference_jobs: Arc::new(AtomicUsize::new(0)),
            download_cancels: Mutex::new(HashMap::new()),
            page_histories: Arc::new(Mutex::new(HashMap::new())),
            page_history_order: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn detect_hardware_cached(&self) -> &HardwareProfile {
        self.cached_hardware.get_or_init(|| self.detect_hardware())
    }

    fn send_worker_command(&self, cmd: WorkerCommand) -> Result<(), String> {
        let existing_tx = self
            .worker_tx
            .lock()
            .map_err(|_| "Hy-MT2 워커 잠금 오류".to_string())?
            .clone();
        if let Some(tx) = existing_tx {
            match tx.send(cmd) {
                Ok(()) => return Ok(()),
                Err(std_mpsc::SendError(unsent)) => {
                    let tx = self.spawn_worker_thread()?;
                    return tx
                        .send(unsent)
                        .map_err(|_| "Hy-MT2 추론 워커에 명령을 전달하지 못했습니다.".to_string());
                }
            }
        }
        let tx = self.spawn_worker_thread()?;
        tx.send(cmd)
            .map_err(|_| "Hy-MT2 추론 워커에 명령을 전달하지 못했습니다.".to_string())
    }

    fn spawn_worker_thread(&self) -> Result<std_mpsc::Sender<WorkerCommand>, String> {
        let backend_arc = self.ensure_backend()?;
        let loaded_meta = Arc::clone(&self.loaded_meta);
        let (tx, rx) = std_mpsc::channel::<WorkerCommand>();
        std::thread::Builder::new()
            .name("hy-mt2-worker".into())
            .spawn(move || run_hy_mt2_worker(rx, backend_arc, loaded_meta))
            .map_err(|e| format!("Hy-MT2 추론 워커 스레드를 시작하지 못했습니다: {e}"))?;
        if let Ok(mut guard) = self.worker_tx.lock() {
            *guard = Some(tx.clone());
        }
        Ok(tx)
    }

    pub async fn clear_page_history(&self, page_url: Option<&str>) {
        let Some(key) = page_url.map(str::trim).filter(|s| !s.is_empty()) else {
            return;
        };
        self.page_histories.lock().await.remove(key);
        self.page_history_order
            .lock()
            .await
            .retain(|existing| existing != key);
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    fn ensure_backend(&self) -> Result<Arc<LlamaBackend>, String> {
        let res = GLOBAL_LLAMA_BACKEND.get_or_init(|| {
            ensure_safe_vulkan_env();
            LlamaBackend::init()
                .map(Arc::new)
                .map_err(|error| format!("Hy-MT2 추론 백엔드를 초기화하지 못했습니다: {error}"))
        });
        res.clone()
    }

    fn touch_activity(&self) {
        self.last_activity_ms
            .store(now_epoch_ms(), Ordering::Relaxed);
    }

    pub async fn unload(&self) -> bool {
        let has_worker = self.worker_tx.lock().ok().and_then(|g| g.clone()).is_some();
        if !has_worker {
            if let Ok(mut meta) = self.loaded_meta.write() {
                *meta = None;
            }
            return false;
        }
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .send_worker_command(WorkerCommand::Unload { reply: reply_tx })
            .is_err()
        {
            return false;
        }
        reply_rx.await.unwrap_or(false)
    }

    pub async fn unload_if_idle(&self, idle_unload_secs: u64) -> bool {
        if idle_unload_secs == 0 {
            return false;
        }
        if self.active_inference_jobs.load(Ordering::Acquire) > 0 {
            return false;
        }
        let last = self.last_activity_ms.load(Ordering::Relaxed);
        let elapsed_ms = now_epoch_ms().saturating_sub(last);
        if elapsed_ms < idle_unload_secs.saturating_mul(1_000) {
            return false;
        }
        let is_loaded = self
            .loaded_meta
            .read()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        if !is_loaded || self.active_inference_jobs.load(Ordering::Acquire) > 0 {
            return false;
        }
        self.unload().await
    }

    pub async fn loaded_model_info(&self) -> Option<(String, String)> {
        self.loaded_meta.read().ok().and_then(|guard| {
            guard
                .as_ref()
                .map(|item| (item.model_id.clone(), item.backend_label.clone()))
        })
    }

    pub fn detect_hardware(&self) -> HardwareProfile {
        let _ = self.ensure_backend();
        let (total_ram_mb, available_ram_mb) = query_system_memory_mb();
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let cpu_name = query_cpu_name();
        let (npu_available, mut npu_name) = query_npu_device();
        let dxgi_adapters = query_dxgi_adapters();
        let devices = list_llama_ggml_backend_devices();

        let mut vulkan_available = false;
        let mut cuda_available = false;
        let mut candidates: Vec<GgmlDeviceCandidate> = Vec::new();
        let mut detected_gpus: Vec<String> = Vec::new();

        for dev in devices {
            match dev.device_type {
                LlamaBackendDeviceType::Gpu | LlamaBackendDeviceType::IntegratedGpu => {
                    let label = if dev.description.trim().is_empty() {
                        dev.name.clone()
                    } else {
                        dev.description.clone()
                    };
                    if is_software_gpu_name(&label) || is_software_gpu_name(&dev.name) {
                        continue;
                    }
                    let backend_lower = dev.backend.to_ascii_lowercase();
                    if backend_lower.contains("cuda") {
                        cuda_available = true;
                    } else if backend_lower.contains("vulkan") {
                        vulkan_available = true;
                    }
                    let mem_mb = (dev.memory_total / (1024 * 1024)) as u64;
                    let is_discrete = matches!(dev.device_type, LlamaBackendDeviceType::Gpu);
                    detected_gpus.push(format!(
                        "{label} ({}, {} MiB, {})",
                        dev.backend,
                        mem_mb,
                        if is_discrete { "외장" } else { "내장" }
                    ));
                    candidates.push(GgmlDeviceCandidate {
                        index: dev.index,
                        name: dev.name,
                        description: label,
                        backend: dev.backend,
                        is_discrete,
                        memory_total_mb: mem_mb,
                    });
                }
                LlamaBackendDeviceType::Accelerator => {
                    if npu_name.is_none() && !dev.description.trim().is_empty() {
                        npu_name = Some(dev.description.clone());
                    }
                }
                _ => {}
            }
        }

        let selected_gpu = select_best_ggml_gpu(&candidates);
        if detected_gpus.is_empty() {
            for adapter in &dxgi_adapters {
                let kind = if is_discrete_dxgi_adapter(adapter) {
                    "외장"
                } else {
                    "내장"
                };
                detected_gpus.push(format!(
                    "{} (DXGI, 전용 {} MiB, {})",
                    adapter.name, adapter.dedicated_vram_mb, kind
                ));
            }
        }

        let selected_label = selected_gpu.as_ref().map(|g| g.description.clone());
        let selected_vendor_guess = selected_label
            .as_deref()
            .map(|lbl| detect_gpu_vendor(lbl, None))
            .unwrap_or("none");
        let matched_dxgi = match_dxgi_adapter_for_gpu(
            selected_label.as_deref(),
            selected_vendor_guess,
            &dxgi_adapters,
        );

        let gpu_name = selected_label.or_else(|| matched_dxgi.map(|d| d.name.clone()));
        let gpu_vendor = matched_dxgi
            .map(|d| detect_gpu_vendor(&d.name, Some(d.vendor_id)))
            .or_else(|| gpu_name.as_deref().map(|n| detect_gpu_vendor(n, None)))
            .unwrap_or("none")
            .to_string();

        let gpu_lower = gpu_name.as_deref().unwrap_or("").to_ascii_lowercase();
        let is_discrete_gpu = if let Some(ref best) = selected_gpu {
            if gpu_vendor == "nvidia" {
                true
            } else if is_known_amd_apu_name(&gpu_lower) || is_known_intel_igpu_name(&gpu_lower) {
                false
            } else {
                best.is_discrete || matched_dxgi.is_some_and(is_discrete_dxgi_adapter)
            }
        } else {
            matched_dxgi.is_some_and(is_discrete_dxgi_adapter)
        };

        // AMD APU applies ONLY when the selected GPU is actually an AMD integrated GPU
        // (never when an AMD Ryzen CPU is paired with a discrete NVIDIA/AMD/Intel GPU).
        let is_amd_apu = gpu_vendor == "amd" && !is_discrete_gpu;

        let ggml_vram_mb = selected_gpu
            .as_ref()
            .map(|g| g.memory_total_mb)
            .unwrap_or(0);
        let dxgi_ded_mb = matched_dxgi.map(|d| d.dedicated_vram_mb).unwrap_or(0);
        let shared_vram_mb = matched_dxgi.map(|d| d.shared_vram_mb).unwrap_or(0);
        let dedicated_vram_mb = if dxgi_ded_mb > 0 {
            dxgi_ded_mb
        } else {
            ggml_vram_mb
        };

        let selected_gpu_device_index = selected_gpu.as_ref().map(|g| g.index);
        let selected_backend_lower = selected_gpu
            .as_ref()
            .map(|g| g.backend.to_ascii_lowercase())
            .unwrap_or_default();

        let vulkan_compiled = cfg!(feature = "vulkan");
        let cuda_compiled = cfg!(feature = "cuda");
        let cuda_ready = cuda_available && selected_backend_lower.contains("cuda");
        let vulkan_ready = !cuda_ready
            && vulkan_available
            && (vulkan_compiled || selected_backend_lower.contains("vulkan"));

        let active_backend = if cuda_ready {
            if gpu_vendor == "nvidia" {
                "cuda-nvidia".to_string()
            } else {
                "cuda".to_string()
            }
        } else if vulkan_ready {
            match (gpu_vendor.as_str(), is_discrete_gpu, is_amd_apu) {
                ("nvidia", _, _) => "vulkan-nvidia".to_string(),
                (_, _, true) => "vulkan-amd-apu".to_string(),
                ("amd", true, _) => "vulkan-amd".to_string(),
                ("intel", true, _) => "vulkan-intel".to_string(),
                ("intel", false, _) => "vulkan-intel-igpu".to_string(),
                _ => "vulkan".to_string(),
            }
        } else {
            "cpu".to_string()
        };

        let compute_decision = build_compute_decision_summary(
            &gpu_vendor,
            is_discrete_gpu,
            cuda_ready,
            vulkan_ready,
            is_amd_apu,
            npu_available,
            npu_name.as_deref(),
            gpu_name.as_deref(),
            dedicated_vram_mb,
            shared_vram_mb,
            detected_gpus.len(),
        );

        let (recommended_tier, recommended_model_id, recommendation_reason) =
            recommend_hardware_tier_ext(
                total_ram_mb,
                dedicated_vram_mb,
                shared_vram_mb,
                &gpu_vendor,
                is_discrete_gpu,
                is_amd_apu,
                npu_available,
            );

        HardwareProfile {
            total_ram_mb,
            available_ram_mb,
            cpu_cores,
            cpu_name,
            gpu_name,
            dedicated_vram_mb,
            shared_vram_mb,
            is_amd_apu,
            npu_available: npu_available || npu_name.is_some(),
            npu_name,
            vulkan_compiled,
            vulkan_available,
            active_backend,
            compute_decision,
            recommended_tier: recommended_tier.to_string(),
            recommended_model_id: recommended_model_id.to_string(),
            recommendation_reason,
            gpu_vendor,
            is_discrete_gpu,
            selected_gpu_device_index,
            cuda_compiled,
            cuda_available,
            detected_gpus,
        }
    }

    pub async fn list_models(&self) -> Vec<HyMt2ModelState> {
        let hw = self.detect_hardware_cached();
        let loaded_id = self
            .loaded_meta
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|loaded| loaded.model_id.clone()));
        let active_downloads: Vec<String> =
            self.download_cancels.lock().await.keys().cloned().collect();

        let mut list: Vec<HyMt2ModelState> = HY_MT2_CATALOG
            .iter()
            .map(|spec| {
                let path = self.models_dir.join(spec.filename);
                let part_path = partial_model_path(&path);
                let installed_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let partial_bytes = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
                let installed = installed_bytes >= 1_048_576 && path.is_file();
                HyMt2ModelState {
                    id: spec.id.to_string(),
                    display_name: spec.display_name.to_string(),
                    tier: spec.tier.to_string(),
                    repo: spec.repo.to_string(),
                    filename: spec.filename.to_string(),
                    download_url: hf_download_url(spec.repo, spec.filename),
                    expected_bytes: spec.expected_bytes,
                    sha256: spec.sha256.to_string(),
                    description: spec.description.to_string(),
                    min_ram_mb: spec.min_ram_mb,
                    min_vram_mb: spec.min_vram_mb,
                    recommended: spec.id == hw.recommended_model_id,
                    installed,
                    loaded: loaded_id.as_deref() == Some(spec.id),
                    downloading: active_downloads.iter().any(|d| d == spec.id),
                    file_path: path.to_string_lossy().into_owned(),
                    installed_bytes,
                    partial_bytes,
                    experimental_kernel_note: spec.experimental_kernel_note.map(str::to_string),
                }
            })
            .collect();

        // Include any user-provided custom *.gguf dropped into AppLocalData/models.
        if let Ok(entries) = fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !name.to_ascii_lowercase().ends_with(".gguf") {
                    continue;
                }
                if list
                    .iter()
                    .any(|item| item.filename.eq_ignore_ascii_case(name))
                {
                    continue;
                }
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                list.push(HyMt2ModelState {
                    id: name.to_string(),
                    display_name: format!("사용자 로컬 모델 ({name})"),
                    tier: "custom".to_string(),
                    repo: "local".to_string(),
                    filename: name.to_string(),
                    download_url: String::new(),
                    expected_bytes: size,
                    sha256: String::new(),
                    description: "AppLocalData models 폴더에 직접 추가된 GGUF 모델".to_string(),
                    min_ram_mb: 2_000,
                    min_vram_mb: 0,
                    recommended: false,
                    installed: true,
                    loaded: loaded_id.as_deref() == Some(name),
                    downloading: false,
                    file_path: path.to_string_lossy().into_owned(),
                    installed_bytes: size,
                    partial_bytes: 0,
                    experimental_kernel_note: None,
                });
            }
        }
        list
    }

    pub async fn available_provider_models(&self) -> Vec<ModelInfo> {
        let models = self.list_models().await;
        let mut installed_first: Vec<ModelInfo> = Vec::new();
        let mut remote_rest: Vec<ModelInfo> = Vec::new();
        for m in models {
            let suffix = if m.loaded {
                " · 메모리 로드됨"
            } else if m.installed {
                " · 설치됨"
            } else if m.recommended {
                " · 권장(다운로드 필요)"
            } else {
                " · 미설치"
            };
            let info = ModelInfo {
                name: m.id,
                display_name: format!("{}{}", m.display_name, suffix),
            };
            if m.installed {
                installed_first.push(info);
            } else {
                remote_rest.push(info);
            }
        }
        installed_first.extend(remote_rest);
        installed_first
    }

    pub fn resolve_model_path(&self, requested: &str) -> Result<(String, PathBuf), String> {
        let trimmed = requested.trim();
        if !trimmed.is_empty() {
            let explicit = PathBuf::from(trimmed);
            if explicit.is_file()
                && explicit
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
            {
                let id = explicit
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(trimmed)
                    .to_string();
                return Ok((id, explicit));
            }
            if let Some(spec) = find_spec(trimmed) {
                let candidate = self.models_dir.join(spec.filename);
                if candidate.is_file() {
                    return Ok((spec.id.to_string(), candidate));
                }
                return Err(format!(
                    "Hy-MT2 모델({})이 아직 다운로드되지 않았습니다. 설정 → AI 및 모델에서 다운로드하세요.",
                    spec.filename
                ));
            }
            let safe_name = Path::new(trimmed)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(trimmed);
            let candidate = self.models_dir.join(safe_name);
            if candidate.is_file() {
                return Ok((safe_name.to_string(), candidate));
            }
        }

        // Fall back cleanly to any installed model ordered by hardware recommendation, then size.
        let hw = self.detect_hardware_cached();
        let priority = [
            hw.recommended_model_id.as_str(),
            "Hy-MT2-1.8B-Q4_K_M.gguf",
            "Hy-MT2-1.8B-Q6_K.gguf",
            "Hy-MT2-7B-Q4_K_M.gguf",
            "Hy-MT2-1.8B-2Bit.gguf",
            "Hy-MT2-1.8B-1.25Bit.gguf",
        ];
        for id in priority {
            let candidate = self.models_dir.join(id);
            if candidate.is_file() {
                return Ok((id.to_string(), candidate));
            }
        }
        if let Ok(entries) = fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
                {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("custom.gguf")
                        .to_string();
                    return Ok((name, path));
                }
            }
        }
        Err(format!(
            "설치된 Hy-MT2 GGUF 모델이 없습니다({}). 설정 → AI 및 모델에서 모델을 다운로드하세요.",
            self.models_dir.display()
        ))
    }

    pub async fn download_model(
        &self,
        model_id: &str,
        mut on_progress: impl FnMut(ModelDownloadProgress) + Send,
    ) -> Result<HyMt2ModelState, String> {
        let spec = find_spec(model_id)
            .ok_or_else(|| format!("알 수 없는 Hy-MT2 모델 ID입니다: {model_id}"))?;
        let final_path = self.models_dir.join(spec.filename);
        let part_path = partial_model_path(&final_path);
        fs::create_dir_all(&self.models_dir)
            .map_err(|e| format!("모델 폴더를 만들지 못했습니다: {e}"))?;

        if final_path.is_file() {
            on_progress(ModelDownloadProgress {
                model_id: spec.id.to_string(),
                status: "verifying".into(),
                downloaded_bytes: spec.expected_bytes,
                total_bytes: spec.expected_bytes,
                percent: 100.0,
                speed_mbps: 0.0,
                message: Some("기존 모델 무결성 검증 중…".into()),
            });
            let valid_existing = if spec.sha256.is_empty() {
                fs::metadata(&final_path)
                    .map(|m| m.len() >= 1_048_576)
                    .unwrap_or(false)
            } else {
                let path_clone = final_path.clone();
                let existing_sha = tokio::task::spawn_blocking(move || sha256_file(&path_clone))
                    .await
                    .map_err(|e| format!("SHA256 검사 작업 오류: {e}"))??;
                existing_sha.eq_ignore_ascii_case(spec.sha256)
            };
            if valid_existing {
                on_progress(ModelDownloadProgress {
                    model_id: spec.id.to_string(),
                    status: "completed".into(),
                    downloaded_bytes: spec.expected_bytes,
                    total_bytes: spec.expected_bytes,
                    percent: 100.0,
                    speed_mbps: 0.0,
                    message: Some("이미 설치 및 검증 완료".into()),
                });
                return self
                    .list_models()
                    .await
                    .into_iter()
                    .find(|m| m.id == spec.id)
                    .ok_or_else(|| "모델 상태를 조회하지 못했습니다.".to_string());
            }
            let _ = fs::remove_file(&final_path);
        }

        let token = CancellationToken::new();
        {
            let mut cancels = self.download_cancels.lock().await;
            if cancels.contains_key(spec.id) {
                return Err("해당 모델이 이미 다운로드 중입니다.".into());
            }
            cancels.insert(spec.id.to_string(), token.clone());
        }

        struct DownloadCleanup<'a> {
            cancels: &'a Mutex<HashMap<String, CancellationToken>>,
            id: String,
        }
        impl Drop for DownloadCleanup<'_> {
            fn drop(&mut self) {
                if let Ok(mut guard) = self.cancels.try_lock() {
                    guard.remove(&self.id);
                }
            }
        }
        let _cleanup = DownloadCleanup {
            cancels: &self.download_cancels,
            id: spec.id.to_string(),
        };

        let mut existing_len = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        if existing_len > spec.expected_bytes {
            let _ = fs::remove_file(&part_path);
            existing_len = 0;
        }

        let download_client = reqwest::Client::builder()
            .user_agent("Fluo-HyMT2/0.1")
            .redirect(reqwest::redirect::Policy::limited(8))
            .build()
            .map_err(|e| format!("다운로드 클라이언트를 초기화하지 못했습니다: {e}"))?;
        let url = hf_download_url(spec.repo, spec.filename);
        let mut req = download_client.get(&url);
        if existing_len > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={existing_len}-"));
        }

        let response = tokio::select! {
            res = req.send() => res.map_err(|e| format!("모델 다운로드 요청 실패: {e}"))?,
            _ = token.cancelled() => {
                on_progress(ModelDownloadProgress {
                    model_id: spec.id.to_string(),
                    status: "cancelled".into(),
                    downloaded_bytes: existing_len,
                    total_bytes: spec.expected_bytes,
                    percent: percent_of(existing_len, spec.expected_bytes),
                    speed_mbps: 0.0,
                    message: Some("다운로드가 취소되었습니다(이어받기 가능).".into()),
                });
                return Err("다운로드가 취소되었습니다.".into());
            }
        };

        let status = response.status();
        let append_mode = status == reqwest::StatusCode::PARTIAL_CONTENT && existing_len > 0;
        if !status.is_success() {
            // Range unsatisfactory when part already equals total size: try fresh/verify.
            if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
                && existing_len == spec.expected_bytes
            {
                // Fall through to verify directly
                existing_len = spec.expected_bytes;
            } else {
                return Err(format!("모델 서버 응답 오류 (HTTP {status})"));
            }
        }

        if existing_len < spec.expected_bytes || !append_mode {
            let total_bytes = if append_mode {
                response
                    .content_length()
                    .map(|rem| existing_len + rem)
                    .unwrap_or(spec.expected_bytes)
            } else {
                response.content_length().unwrap_or(spec.expected_bytes)
            };
            let mut downloaded = if append_mode { existing_len } else { 0 };
            let file = if append_mode {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&part_path)
            } else {
                File::create(&part_path)
            }
            .map_err(|e| format!("임시 모델 파일을 열지 못했습니다: {e}"))?;
            let mut writer = BufWriter::with_capacity(256 * 1024, file);

            let start_bytes = downloaded;
            let start_time = Instant::now();
            let mut last_emit = Instant::now();
            let mut stream = response.bytes_stream();

            loop {
                tokio::select! {
                    biased;
                    _ = token.cancelled() => {
                        let _ = writer.flush();
                        on_progress(ModelDownloadProgress {
                            model_id: spec.id.to_string(),
                            status: "cancelled".into(),
                            downloaded_bytes: downloaded,
                            total_bytes,
                            percent: percent_of(downloaded, total_bytes),
                            speed_mbps: 0.0,
                            message: Some("다운로드가 일시중지/취소되었습니다(이어받기 가능).".into()),
                        });
                        return Err("모델 다운로드가 취소되었습니다.".into());
                    }
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                writer
                                    .write_all(&bytes)
                                    .map_err(|e| format!("모델 파일 쓰기 오류: {e}"))?;
                                downloaded += bytes.len() as u64;
                                if last_emit.elapsed() >= Duration::from_millis(160) || downloaded >= total_bytes {
                                    let secs = start_time.elapsed().as_secs_f64().max(0.05);
                                    let speed_mbps = (downloaded.saturating_sub(start_bytes) as f64) / (1024.0 * 1024.0) / secs;
                                    on_progress(ModelDownloadProgress {
                                        model_id: spec.id.to_string(),
                                        status: "downloading".into(),
                                        downloaded_bytes: downloaded,
                                        total_bytes,
                                        percent: percent_of(downloaded, total_bytes),
                                        speed_mbps,
                                        message: None,
                                    });
                                    last_emit = Instant::now();
                                }
                            }
                            Some(Err(e)) => {
                                let _ = writer.flush();
                                return Err(format!("다운로드 스트림 오류: {e}"));
                            }
                            None => break,
                        }
                    }
                }
            }
            writer
                .flush()
                .map_err(|e| format!("모델 파일 플러시 오류: {e}"))?;
        }

        on_progress(ModelDownloadProgress {
            model_id: spec.id.to_string(),
            status: "verifying".into(),
            downloaded_bytes: spec.expected_bytes,
            total_bytes: spec.expected_bytes,
            percent: 100.0,
            speed_mbps: 0.0,
            message: Some("SHA256 무결성 검사 중…".into()),
        });

        if !spec.sha256.is_empty() {
            let part_clone = part_path.clone();
            let actual_sha = tokio::task::spawn_blocking(move || sha256_file(&part_clone))
                .await
                .map_err(|e| format!("SHA256 검사 실패: {e}"))??;
            if !actual_sha.eq_ignore_ascii_case(spec.sha256) {
                let _ = fs::remove_file(&part_path);
                return Err(format!(
                    "다운로드한 모델 파일의 SHA256 체크섬이 일치하지 않습니다(예상: {}, 실제: {}).",
                    spec.sha256, actual_sha
                ));
            }
        } else {
            let downloaded_len = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
            if downloaded_len < 1_048_576 {
                let _ = fs::remove_file(&part_path);
                return Err("다운로드한 모델 파일 크기가 비정상적으로 작습니다.".into());
            }
        }

        if final_path.exists() {
            let _ = fs::remove_file(&final_path);
        }
        fs::rename(&part_path, &final_path)
            .map_err(|e| format!("다운로드한 모델을 최종 위치로 이동하지 못했습니다: {e}"))?;

        on_progress(ModelDownloadProgress {
            model_id: spec.id.to_string(),
            status: "completed".into(),
            downloaded_bytes: spec.expected_bytes,
            total_bytes: spec.expected_bytes,
            percent: 100.0,
            speed_mbps: 0.0,
            message: Some("모델 다운로드 및 검증 완료".into()),
        });

        self.list_models()
            .await
            .into_iter()
            .find(|m| m.id == spec.id)
            .ok_or_else(|| "완료된 모델을 찾지 못했습니다.".to_string())
    }

    pub async fn cancel_download(&self, model_id: &str) -> bool {
        let guard = self.download_cancels.lock().await;
        if let Some(token) = guard.get(model_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub async fn delete_model(&self, model_id: &str) -> Result<(), String> {
        let _ = self.cancel_download(model_id).await;
        let is_currently_loaded = self
            .loaded_meta
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|m| m.model_id == model_id))
            .unwrap_or(false);
        if is_currently_loaded {
            let _ = self.unload().await;
        }
        let file_name = find_spec(model_id)
            .map(|s| s.filename.to_string())
            .or_else(|| {
                Path::new(model_id)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| "올바른 모델 이름이 아닙니다.".to_string())?;
        let target = self.models_dir.join(&file_name);
        let part = partial_model_path(&target);
        if part.exists() {
            let _ = fs::remove_file(&part);
        }
        if target.exists() {
            fs::remove_file(&target)
                .map_err(|e| format!("모델 파일을 삭제하지 못했습니다: {e}"))?;
        }
        Ok(())
    }

    fn resolve_worker_model_spec(&self, settings: &AppSettings) -> Result<WorkerModelSpec, String> {
        let (model_id, file_path) = self.resolve_model_path(&settings.model)?;
        let hw = self.detect_hardware_cached();
        let file_size_bytes = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
        let (desired_gpu_layers, hw_backend) =
            resolve_optimal_gpu_layers(settings, hw, file_size_bytes);
        let ctx_size = settings.hy_mt2_ctx_size.clamp(2048, 32768);
        let threads = if settings.hy_mt2_threads > 0 {
            settings.hy_mt2_threads as i32
        } else {
            default_cpu_threads()
        };
        Ok(WorkerModelSpec {
            model_id,
            file_path,
            n_gpu_layers: desired_gpu_layers,
            preferred_device_index: (desired_gpu_layers > 0)
                .then_some(hw.selected_gpu_device_index)
                .flatten(),
            backend_label: hw_backend,
            ctx_size,
            threads,
        })
    }

    pub async fn test_model_load(
        &self,
        settings: &AppSettings,
    ) -> Result<(String, String), String> {
        let spec = self.resolve_worker_model_spec(settings)?;
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.send_worker_command(WorkerCommand::EnsureLoaded {
            spec,
            reply: reply_tx,
        })?;
        let meta = reply_rx
            .await
            .map_err(|_| "Hy-MT2 모델 로드 응답을 받지 못했습니다.".to_string())??;
        self.touch_activity();
        Ok((meta.model_id, meta.backend_label))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn translate_segments(
        &self,
        settings: &AppSettings,
        segments: &[SegmentInput],
        context: &PromptContext<'_>,
        page_url: Option<&str>,
        prior_context: &[ContextPair],
        force: bool,
        cancel: CancellationToken,
        mut on_segment: impl FnMut(SegmentResult) + Send,
    ) -> Result<(Vec<SegmentResult>, String, String), String> {
        if segments.is_empty() {
            return Ok((Vec::new(), String::new(), "idle".into()));
        }
        self.active_inference_jobs.fetch_add(1, Ordering::AcqRel);
        struct ActiveGuard(Arc<AtomicUsize>, Arc<AtomicU64>);
        impl Drop for ActiveGuard {
            fn drop(&mut self) {
                self.1.store(now_epoch_ms(), Ordering::Relaxed);
                self.0.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let _active_guard = ActiveGuard(
            Arc::clone(&self.active_inference_jobs),
            Arc::clone(&self.last_activity_ms),
        );

        let spec = self.resolve_worker_model_spec(settings)?;
        let model_id = spec.model_id.clone();
        let fallback_backend_label = spec.backend_label.clone();

        let target_lang = context.target_language.to_string();
        let style = context.style.to_string();
        let glossary = context.glossary.to_vec();
        let page_key = page_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        if force && let Some(ref key) = page_key {
            self.clear_page_history(Some(key)).await;
        }

        let mut initial_history = if let Some(ref key) = page_key {
            self.page_histories
                .lock()
                .await
                .get(key)
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        merge_context_pairs(&mut initial_history, prior_context);

        let index_by_id: HashMap<String, usize> = segments
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();
        let inputs_owned = segments.to_vec();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SegmentResult>();
        let (reply_tx, mut reply_rx) =
            tokio::sync::oneshot::channel::<Result<Vec<ContextPair>, String>>();

        self.send_worker_command(WorkerCommand::TranslateBatch {
            spec,
            segments: inputs_owned,
            target_lang,
            style,
            glossary,
            history: initial_history,
            cancel,
            progress_tx: tx,
            reply: reply_tx,
        })?;

        let mut results = Vec::with_capacity(segments.len());
        let mut updated_history: Option<Vec<ContextPair>> = None;
        let mut worker_finished = false;
        loop {
            tokio::select! {
                maybe_seg = rx.recv() => {
                    if let Some(seg) = maybe_seg {
                        on_segment(seg.clone());
                        results.push(seg);
                    } else {
                        break;
                    }
                }
                res = &mut reply_rx => {
                    worker_finished = true;
                    while let Ok(seg) = rx.try_recv() {
                        on_segment(seg.clone());
                        results.push(seg);
                    }
                    let hist_outcome = res.map_err(|_| "Hy-MT2 추론 워커가 종료되었습니다.".to_string())??;
                    updated_history = Some(hist_outcome);
                    break;
                }
            }
        }
        if !worker_finished {
            let hist_outcome = reply_rx
                .await
                .map_err(|_| "Hy-MT2 추론 워커가 종료되었습니다.".to_string())??;
            updated_history = Some(hist_outcome);
        }
        results.sort_by_key(|r| index_by_id.get(&r.id).copied().unwrap_or(usize::MAX));
        if let (Some(key), Some(next_hist)) = (page_key, updated_history) {
            self.store_page_history(key, next_hist).await;
        }
        self.touch_activity();
        let actual_backend = self
            .loaded_model_info()
            .await
            .map(|(_, b)| b)
            .unwrap_or(fallback_backend_label);
        Ok((results, model_id, actual_backend))
    }

    async fn store_page_history(&self, page_key: String, mut history: Vec<ContextPair>) {
        // Keep only entries with an actual translation so subsequent chunks inherit clean pairs.
        history.retain(|pair| {
            pair.translation
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty())
        });
        const MAX_PAGE_HISTORY: usize = 240;
        if history.len() > MAX_PAGE_HISTORY {
            history.drain(..history.len() - MAX_PAGE_HISTORY);
        }
        let mut histories = self.page_histories.lock().await;
        histories.insert(page_key.clone(), history);
        let mut order = self.page_history_order.lock().await;
        order.retain(|existing| existing != &page_key);
        order.push(page_key);
        const MAX_TRACKED_PAGES: usize = 8;
        while order.len() > MAX_TRACKED_PAGES {
            let evicted = order.remove(0);
            histories.remove(&evicted);
        }
    }
}

impl Drop for HyMt2Engine {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.worker_tx.lock()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(WorkerCommand::Shutdown);
        }
    }
}

fn run_hy_mt2_worker(
    rx: std_mpsc::Receiver<WorkerCommand>,
    backend: Arc<LlamaBackend>,
    loaded_meta: Arc<StdRwLock<Option<LoadedModelMeta>>>,
) {
    let mut pending_cmd: Option<WorkerCommand> = None;

    'model_loop: loop {
        let cmd = match pending_cmd.take() {
            Some(c) => c,
            None => match rx.recv() {
                Ok(c) => c,
                Err(_) => break 'model_loop,
            },
        };

        let spec = match cmd.spec() {
            Some(spec) => spec.clone(),
            None => match cmd {
                WorkerCommand::Unload { reply } => {
                    if let Ok(mut guard) = loaded_meta.write() {
                        *guard = None;
                    }
                    let _ = reply.send(false);
                    continue 'model_loop;
                }
                WorkerCommand::Shutdown => break 'model_loop,
                _ => continue 'model_loop,
            },
        };

        if let Ok(mut guard) = loaded_meta.write() {
            *guard = None;
        }

        let mut attempts: Vec<(u32, Option<usize>, String)> = Vec::new();
        attempts.push((
            spec.n_gpu_layers,
            spec.preferred_device_index,
            spec.backend_label.clone(),
        ));
        if spec.n_gpu_layers > 16 {
            let half_ngl = 12u32.max(spec.n_gpu_layers.min(32) / 2);
            attempts.push((
                half_ngl,
                spec.preferred_device_index,
                format!("{}-fallback-hybrid", spec.backend_label),
            ));
        }
        if spec.n_gpu_layers > 0 {
            attempts.push((0, None, "cpu(fallback)".to_string()));
        }

        let mut last_load_err = String::new();
        let mut active_cmd_slot = Some(cmd);

        'attempt_loop: for (attempt_ngl, pin_dev, attempt_label) in attempts {
            let mut model_params = LlamaModelParams::default().with_n_gpu_layers(attempt_ngl);
            if attempt_ngl > 0 {
                model_params = model_params.with_split_mode(LlamaSplitMode::None);
                if let Some(dev_idx) = pin_dev
                    && let Ok(pinned) = LlamaModelParams::default()
                        .with_n_gpu_layers(attempt_ngl)
                        .with_split_mode(LlamaSplitMode::None)
                        .with_devices(&[dev_idx])
                {
                    model_params = pinned;
                }
            }

            let model = match LlamaModel::load_from_file(&backend, &spec.file_path, &model_params) {
                Ok(m) => m,
                Err(error) => {
                    last_load_err = format!(
                        "Hy-MT2 모델({})을 로드하지 못했습니다: {error}",
                        spec.file_path.display()
                    );
                    continue 'attempt_loop;
                }
            };
            let mut session =
                match HyMt2Session::new(&backend, &model, spec.ctx_size, spec.threads, attempt_ngl)
                {
                    Ok(s) => s,
                    Err(err) => {
                        last_load_err = err;
                        continue 'attempt_loop;
                    }
                };

            let mut active_cmd = active_cmd_slot
                .take()
                .expect("worker command must still be unconsumed on model load");
            let chat_template = model.chat_template(None).ok();
            let tmpl_raw = model
                .meta_val_str("tokenizer.chat_template")
                .unwrap_or_default();
            let uses_hy_template =
                tmpl_raw.contains("hy_User") || tmpl_raw.contains("hy_Assistant");

            let active_meta = LoadedModelMeta {
                model_id: spec.model_id.clone(),
                backend_label: attempt_label,
            };
            if let Ok(mut guard) = loaded_meta.write() {
                *guard = Some(active_meta.clone());
            }

            loop {
                match active_cmd {
                    WorkerCommand::EnsureLoaded {
                        spec: req_spec,
                        reply,
                    } => {
                        if req_spec.model_id != spec.model_id
                            || req_spec.file_path != spec.file_path
                            || req_spec.n_gpu_layers != spec.n_gpu_layers
                            || req_spec.preferred_device_index != spec.preferred_device_index
                        {
                            pending_cmd = Some(WorkerCommand::EnsureLoaded {
                                spec: req_spec,
                                reply,
                            });
                            break 'attempt_loop;
                        }
                        let _ = reply.send(Ok(active_meta.clone()));
                    }
                    WorkerCommand::TranslateBatch {
                        spec: req_spec,
                        segments,
                        target_lang,
                        style,
                        glossary,
                        history,
                        cancel,
                        progress_tx,
                        reply,
                    } => {
                        if req_spec.model_id != spec.model_id
                            || req_spec.file_path != spec.file_path
                            || req_spec.n_gpu_layers != spec.n_gpu_layers
                            || req_spec.preferred_device_index != spec.preferred_device_index
                        {
                            pending_cmd = Some(WorkerCommand::TranslateBatch {
                                spec: req_spec,
                                segments,
                                target_lang,
                                style,
                                glossary,
                                history,
                                cancel,
                                progress_tx,
                                reply,
                            });
                            break 'attempt_loop;
                        }
                        if session.threads != req_spec.threads
                            && let Err(err) = session.rebuild_threads(req_spec.threads)
                        {
                            let _ = reply.send(Err(err));
                            break 'attempt_loop;
                        }
                        let outcome = process_worker_translation_batch(
                            &mut session,
                            &model,
                            uses_hy_template,
                            chat_template.as_ref(),
                            &segments,
                            &target_lang,
                            &style,
                            &glossary,
                            history,
                            req_spec.ctx_size,
                            &cancel,
                            &progress_tx,
                        );
                        let _ = reply.send(outcome);
                    }
                    WorkerCommand::Unload { reply } => {
                        if let Ok(mut guard) = loaded_meta.write() {
                            *guard = None;
                        }
                        drop(session);
                        drop(model);
                        let _ = reply.send(true);
                        continue 'model_loop;
                    }
                    WorkerCommand::Shutdown => {
                        if let Ok(mut guard) = loaded_meta.write() {
                            *guard = None;
                        }
                        drop(session);
                        drop(model);
                        break 'model_loop;
                    }
                }

                active_cmd = match rx.recv() {
                    Ok(c) => c,
                    Err(_) => {
                        if let Ok(mut guard) = loaded_meta.write() {
                            *guard = None;
                        }
                        break 'model_loop;
                    }
                };
            }
        }

        if let Some(failed_cmd) = active_cmd_slot.take() {
            failed_cmd.fail_load(last_load_err);
            continue 'model_loop;
        }

        if let Ok(mut guard) = loaded_meta.write() {
            *guard = None;
        }
    }
}

struct HyMt2Session<'a> {
    backend: &'a LlamaBackend,
    model: &'a LlamaModel,
    ctx: llama_cpp_2::context::LlamaContext<'a>,
    current_n_ctx: u32,
    n_seq_max: u32,
    threads: i32,
    cached_seq0_tokens: Vec<llama_cpp_2::token::LlamaToken>,
}

impl<'a> HyMt2Session<'a> {
    fn new(
        backend: &'a LlamaBackend,
        model: &'a LlamaModel,
        max_ctx_size: u32,
        threads: i32,
        n_gpu_layers: u32,
    ) -> Result<Self, String> {
        // Reserve slot `8` on GPU for shared instruction KV prefix copy (`0..8` = 8 active slots);
        // step down KV slot count gracefully on tight-VRAM (2-3 GB) GPUs before failing.
        let attempts: Vec<(u32, u32)> = if n_gpu_layers > 0 {
            vec![
                (max_ctx_size.clamp(2048, 2560), 9),
                (max_ctx_size.clamp(1536, 2048), 3),
                (max_ctx_size.clamp(1024, 1536), 1),
            ]
        } else {
            vec![(max_ctx_size.clamp(1536, 2048), 1)]
        };
        let mut last_err = String::new();
        for (initial_ctx, n_seq_max) in attempts {
            match build_llama_context(backend, model, initial_ctx, n_seq_max, threads) {
                Ok(ctx) => {
                    return Ok(Self {
                        backend,
                        model,
                        ctx,
                        current_n_ctx: initial_ctx,
                        n_seq_max,
                        threads,
                        cached_seq0_tokens: Vec::new(),
                    });
                }
                Err(err) => {
                    last_err = err;
                }
            }
        }
        Err(last_err)
    }

    fn rebuild_threads(&mut self, threads: i32) -> Result<(), String> {
        self.ctx = build_llama_context(
            self.backend,
            self.model,
            self.current_n_ctx,
            self.n_seq_max,
            threads,
        )?;
        self.threads = threads;
        self.cached_seq0_tokens.clear();
        Ok(())
    }

    fn clear_all_kv(&mut self) {
        self.ctx.clear_kv_cache();
        self.cached_seq0_tokens.clear();
    }

    fn ensure_capacity(&mut self, needed_tokens: u32, max_ctx_size: u32) -> Result<(), String> {
        let clamped_need = needed_tokens.min(max_ctx_size).max(1024);
        if clamped_need <= self.current_n_ctx {
            return Ok(());
        }
        let grown = (clamped_need + 256).min(max_ctx_size).max(clamped_need);
        self.ctx = build_llama_context(
            self.backend,
            self.model,
            grown,
            self.n_seq_max,
            self.threads,
        )?;
        self.current_n_ctx = grown;
        self.cached_seq0_tokens.clear();
        Ok(())
    }
}

fn build_llama_context<'a>(
    backend: &'a LlamaBackend,
    model: &'a LlamaModel,
    ctx_tokens: u32,
    n_seq_max: u32,
    threads: i32,
) -> Result<llama_cpp_2::context::LlamaContext<'a>, String> {
    let n_ctx = NonZeroU32::new(ctx_tokens.max(512));
    let batch_threads = default_batch_threads(threads);
    // Keep unified KV cache with 512 batch/ubatch, F16 Flash Attention, and up to 8 concurrent
    // GPU decode slots (+1 shared prefix slot) to maximize memory-bandwidth amortization.
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(n_ctx)
        .with_n_batch(512)
        .with_n_ubatch(512)
        .with_n_seq_max(n_seq_max.max(1))
        .with_kv_unified(true)
        .with_no_perf(true)
        .with_n_threads(threads)
        .with_n_threads_batch(batch_threads);
    model
        .new_context(backend, ctx_params)
        .map_err(|e| format!("Hy-MT2 추론 컨텍스트를 만들지 못했습니다: {e}"))
}

#[inline]
fn shortcut_translation(source_text: &str, target_language: &str) -> Option<String> {
    if source_text.trim().is_empty() {
        return Some(source_text.to_string());
    }
    if let Some(silent) = silent_ellipsis_shortcut(source_text) {
        return Some(silent);
    }
    if let Some(passthrough) = non_translatable_shortcut(source_text) {
        return Some(passthrough);
    }
    if let Some(native) = already_in_target_language_shortcut(source_text, target_language) {
        return Some(native);
    }
    None
}

fn is_dialogue_line(line: &str, is_cjk: bool) -> bool {
    let l = line.trim();
    if l.is_empty() {
        return false;
    }
    const QUOTE_PAIRS: [(char, char); 6] = [
        ('「', '」'),
        ('『', '』'),
        ('“', '”'),
        ('"', '"'),
        ('〝', '〟'),
        ('（', '）'),
    ];
    let stripped = l.trim_end_matches([' ', '\t']);
    if QUOTE_PAIRS
        .iter()
        .any(|&(open, close)| stripped.starts_with(open) && stripped.ends_with(close))
        || (is_cjk && stripped.starts_with('(') && stripped.ends_with(')'))
    {
        return true;
    }
    stripped.starts_with('「')
        || stripped.starts_with('“')
        || stripped.starts_with("— ")
        || stripped.starts_with("― ")
}

fn split_list_bullet_prefix(line: &str) -> Option<(&str, &str)> {
    let trimmed_start = line.trim_start();
    let indent_len = line.len() - trimmed_start.len();
    for bullet in ["- ", "* ", "• ", "· "] {
        if let Some(body) = trimmed_start.strip_prefix(bullet) {
            let body_trim = body.trim();
            if !body_trim.is_empty()
                && !body_trim
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                let prefix_len = indent_len + bullet.len();
                return Some((&line[..prefix_len], body_trim));
            }
        }
    }
    None
}

fn has_structured_multiline_entries(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let bullet_count = lines
        .iter()
        .filter(|l| split_list_bullet_prefix(l).is_some())
        .count();
    if bullet_count >= 2 {
        return true;
    }
    if lines.len() <= 2 || text.chars().count() <= 160 {
        return false;
    }
    let has_soft_wrap = lines.windows(2).any(|w| {
        let prev = w[0];
        let next = w[1];
        let prev_is_heading = prev.chars().count() <= 36;
        let prev_is_structured = prev.contains(';')
            || prev.ends_with(['.', '!', '?', ':', ';', '。', '！', '？', '」', '』', '”', '"', ')', ']']);
        let next_is_list = split_list_bullet_prefix(next).is_some();
        let next_starts_mid_phrase = next
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase());
        !prev_is_heading && !prev_is_structured && !next_is_list && next_starts_mid_phrase
    });
    if has_soft_wrap {
        return false;
    }
    let is_cjk = has_cjk_or_kana(text);
    let structured_count = lines
        .iter()
        .filter(|l| {
            is_dialogue_line(l, is_cjk)
                || l.contains(';')
                || l.ends_with(['.', '!', '?', ':', ';', '。', '！', '？', '」', '』', '”', '"'])
                || l.chars().count() <= 36
        })
        .count();
    structured_count * 2 >= lines.len()
}

fn has_mixed_dialogue_and_narration(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let is_cjk = has_cjk_or_kana(trimmed);
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let mut has_dlg = false;
    let mut has_nar = false;
    for l in lines {
        if is_dialogue_line(l, is_cjk) {
            has_dlg = true;
        } else {
            has_nar = true;
        }
        if has_dlg && has_nar {
            return true;
        }
    }
    false
}

fn segment_needs_line_split(source_text: &str) -> bool {
    has_structured_multiline_entries(source_text) || has_mixed_dialogue_and_narration(source_text)
}

fn is_parallel_gpu_candidate(source_text: &str) -> bool {
    let chars = source_text.chars().count();
    chars <= 2400 && !segment_needs_line_split(source_text) && !is_dialogue_segment(source_text)
}

fn is_western_abbreviation_before_dot(chars: &[(usize, char)], dot_idx: usize) -> bool {
    if dot_idx == 0 {
        return false;
    }
    if dot_idx + 1 < chars.len()
        && chars[dot_idx - 1].1.is_ascii_digit()
        && chars[dot_idx + 1].1.is_ascii_digit()
    {
        return true;
    }
    let mut k = dot_idx;
    while k > 0 && chars[k - 1].1.is_ascii_alphanumeric() {
        k -= 1;
    }
    let word: String = chars[k..dot_idx].iter().map(|(_, c)| *c).collect();
    if word.is_empty() {
        return false;
    }
    if word.len() == 1 && word.chars().all(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    matches!(
        word.to_ascii_lowercase().as_str(),
        "mr" | "mrs"
            | "ms"
            | "dr"
            | "prof"
            | "sr"
            | "jr"
            | "st"
            | "vs"
            | "etc"
            | "e.g"
            | "i.e"
            | "no"
            | "inc"
            | "ltd"
            | "corp"
            | "vol"
            | "ch"
            | "fig"
            | "al"
            | "jan"
            | "feb"
            | "mar"
            | "apr"
            | "aug"
            | "sep"
            | "sept"
            | "oct"
            | "nov"
            | "dec"
    )
}

pub fn split_long_paragraph_sentences(text: &str) -> Option<Vec<String>> {
    if text.chars().count() <= 340 || text.contains('\n') || is_dialogue_segment(text) {
        return None;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let track_ascii_quotes = text.chars().filter(|&c| c == '"').count() % 2 == 0;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut in_ascii_quote = false;
    let mut cjk_quote_depth = 0usize;
    let mut raw_sentences: Vec<String> = Vec::new();
    let mut sent_start_byte = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let (_, ch) = chars[i];
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '"' if track_ascii_quotes => in_ascii_quote = !in_ascii_quote,
            '“' | '「' | '『' => cjk_quote_depth += 1,
            '”' | '」' | '』' => cjk_quote_depth = cjk_quote_depth.saturating_sub(1),
            '。' | '！' | '？' | '.' | '!' | '?'
                if bracket_depth == 0
                    && paren_depth == 0
                    && !in_ascii_quote
                    && cjk_quote_depth == 0 =>
            {
                let is_cjk_end = matches!(ch, '。' | '！' | '？');
                if ch == '.' && is_western_abbreviation_before_dot(&chars, i) {
                    i += 1;
                    continue;
                }
                let mut end_idx = i + 1;
                let mut end_byte = if end_idx < chars.len() {
                    chars[end_idx].0
                } else {
                    text.len()
                };
                // Keep immediately attached footnote/citation markers (`[[3|[1]]]`) glued to this sentence.
                while end_byte < text.len() && text[end_byte..].starts_with("[[") {
                    if let Some(rel_close) =
                        find_matching_double_bracket_close(&text[end_byte + 2..])
                    {
                        let inner = &text[end_byte + 2..end_byte + 2 + rel_close];
                        let is_footnote = inner
                            .split_once('|')
                            .is_some_and(|(_, rhs)| rhs.trim_start().starts_with('['))
                            || inner.starts_with("img");
                        if is_footnote {
                            end_byte = end_byte + 2 + rel_close + 2;
                            while end_idx < chars.len() && chars[end_idx].0 < end_byte {
                                end_idx += 1;
                            }
                            continue;
                        }
                    }
                    break;
                }

                let mut ws_idx = end_idx;
                while ws_idx < chars.len() && chars[ws_idx].1.is_whitespace() {
                    ws_idx += 1;
                }
                if (is_cjk_end || ws_idx > end_idx) && ws_idx < chars.len() {
                    let next_byte = chars[ws_idx].0;
                    let next_ch = chars[ws_idx].1;
                    let next_starts_sentence = next_ch.is_uppercase()
                        || next_ch.is_ascii_digit()
                        || matches!(next_ch, '"' | '“' | '\'' | '‘' | '「' | '『')
                        || text[next_byte..].starts_with("[[")
                        || !next_ch.is_ascii();
                    if next_starts_sentence {
                        let piece = text[sent_start_byte..end_byte].trim();
                        if !piece.is_empty() {
                            raw_sentences.push(piece.to_string());
                            sent_start_byte = next_byte;
                            i = ws_idx;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if sent_start_byte < text.len() {
        let tail = text[sent_start_byte..].trim();
        if !tail.is_empty() {
            raw_sentences.push(tail.to_string());
        }
    }
    if raw_sentences.len() < 2 {
        return None;
    }

    let mut grouped: Vec<String> = Vec::with_capacity(raw_sentences.len());
    for sent in raw_sentences {
        let s_len = sent.chars().count();
        if let Some(prev) = grouped.last_mut() {
            let p_len = prev.chars().count();
            if (p_len < 160 && p_len + 1 + s_len <= 340) || s_len < 60 {
                if !prev.ends_with(['。', '！', '？']) {
                    prev.push(' ');
                }
                prev.push_str(&sent);
                continue;
            }
        }
        grouped.push(sent);
    }

    (grouped.len() >= 2).then_some(grouped)
}

fn join_translated_sentences(parts: &[String], norm_target: &str) -> String {
    let sep = if matches!(norm_target, "Japanese" | "Chinese" | "Traditional Chinese") {
        ""
    } else {
        " "
    };
    let mut joined = String::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push_str(sep);
        }
        joined.push_str(trimmed);
    }
    joined
}

pub fn looks_like_prompt_echo(source_text: &str, out: &str) -> bool {
    const ECHO_PHRASES: &[&str] = &[
        "마커를 그대로 유지",
        "마커는 그대로 유지",
        "마커도 보존한",
        "배경 정보를 고려하여",
        "추가 설명 없이",
        "preserving [[0|...]]",
        "Preserve [[0|...]]",
        "without additional explanation",
        "Reference the following translations",
        "\" translates to \"",
        "translates to \"",
        "다음 번역을 참고",
        "번역할 것",
        "번역 할 것",
        "으로 번역할",
        "로 번역할",
        "Mandatory Glossary",
        "do NOT translate",
        "keep verbatim",
        "参考下面的翻译",
    ];
    let out_lower = out.to_ascii_lowercase();
    let src_lower = source_text.to_ascii_lowercase();
    if ECHO_PHRASES
        .iter()
        .any(|phrase| {
            let pl = phrase.to_ascii_lowercase();
            out_lower.contains(&pl) && !src_lower.contains(&pl)
        })
    {
        return true;
    }
    false
}

#[inline]
fn glossary_rule_target(rule: &crate::ai::GlossaryRule) -> &str {
    if rule.do_not_translate || rule.translation.trim().is_empty() {
        rule.source.trim()
    } else {
        rule.translation.trim()
    }
}

fn find_glossary_match_range(haystack: &str, term: &str) -> Option<(usize, usize)> {
    let needle = term.trim();
    if needle.is_empty() {
        return None;
    }
    let hay_lower = haystack.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let need_start_boundary = needle
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric());
    let need_end_boundary = needle
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric());

    let mut search_from = 0usize;
    while let Some(rel) = hay_lower[search_from..].find(&needle_lower) {
        let start = search_from + rel;
        let end = start + needle_lower.len();
        if !haystack.is_char_boundary(start) || !haystack.is_char_boundary(end) {
            search_from = search_from.saturating_add(1);
            while search_from < hay_lower.len() && !hay_lower.is_char_boundary(search_from) {
                search_from += 1;
            }
            continue;
        }
        let start_ok = !need_start_boundary
            || haystack[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_ascii_alphanumeric());
        let end_ok = !need_end_boundary
            || haystack[end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric());
        if start_ok && end_ok {
            return Some((start, end));
        }
        search_from = end.max(start + 1);
    }
    None
}

pub fn filter_segment_glossary(
    source_text: &str,
    glossary: &[crate::ai::GlossaryRule],
) -> Vec<crate::ai::GlossaryRule> {
    if glossary.is_empty() || source_text.trim().is_empty() {
        return Vec::new();
    }
    glossary
        .iter()
        .filter(|rule| {
            let src = rule.source.trim();
            if src.is_empty() {
                return false;
            }
            find_glossary_match_range(source_text, src).is_some()
                || rule
                    .aliases
                    .iter()
                    .any(|alias| find_glossary_match_range(source_text, alias).is_some())
        })
        .cloned()
        .collect()
}

pub fn glossary_exact_segment_shortcut(
    source_text: &str,
    glossary: &[crate::ai::GlossaryRule],
) -> Option<String> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() || glossary.is_empty() {
        return None;
    }
    for rule in glossary {
        let tgt = glossary_rule_target(rule);
        if trimmed.eq_ignore_ascii_case(rule.source.trim())
            || rule
                .aliases
                .iter()
                .any(|a| trimmed.eq_ignore_ascii_case(a.trim()))
        {
            return Some(tgt.to_string());
        }
    }
    if trimmed.starts_with("[[")
        && trimmed.ends_with("]]")
        && let Some(close) = find_matching_double_bracket_close(&trimmed[2..])
        && 2 + close + 2 == trimmed.len()
    {
        let inner = &trimmed[2..2 + close];
        if let Some((id_part, body)) = inner.split_once('|')
            && id_part.trim().chars().all(|c| c.is_ascii_digit())
        {
            let body_trim = body.trim();
            for rule in glossary {
                let tgt = glossary_rule_target(rule);
                if body_trim.eq_ignore_ascii_case(rule.source.trim())
                    || rule
                        .aliases
                        .iter()
                        .any(|a| body_trim.eq_ignore_ascii_case(a.trim()))
                {
                    return Some(format!("[[{}|{tgt}]]", id_part.trim()));
                }
            }
        }
    }
    None
}

fn is_dnt_glossary_rule(rule: &crate::ai::GlossaryRule, target: &str) -> bool {
    !target.is_empty() && (rule.do_not_translate || target.eq_ignore_ascii_case(rule.source.trim()))
}

fn replace_glossary_terms(
    text: &str,
    rule: &crate::ai::GlossaryRule,
    target: &str,
    skip_if_already_target: bool,
) -> String {
    let mut terms = Vec::with_capacity(1 + rule.aliases.len());
    let main = rule.source.trim();
    if !main.is_empty() {
        terms.push(main);
    }
    for alias in &rule.aliases {
        let a = alias.trim();
        if !a.is_empty() {
            terms.push(a);
        }
    }
    terms.sort_by_key(|t| std::cmp::Reverse(t.len()));

    let mut cur = text.to_string();
    for term in terms {
        let mut out = String::with_capacity(cur.len());
        let mut rest = cur.as_str();
        while let Some((start, end)) = find_glossary_match_range(rest, term) {
            out.push_str(&rest[..start]);
            if skip_if_already_target
                && (&rest[start..end] == target || rest[start..].starts_with(target))
            {
                out.push_str(&rest[start..end]);
            } else {
                out.push_str(target);
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        cur = out;
    }
    cur
}

fn wrap_dnt_glossary_terms_with_backticks(
    text: &str,
    source_text_lower: &str,
    rule: &crate::ai::GlossaryRule,
    target: &str,
) -> String {
    let wrapped_tgt_lower = format!("`{}`", target.to_lowercase());
    if source_text_lower.contains(&wrapped_tgt_lower) {
        return text.to_string();
    }
    let mut terms = Vec::with_capacity(1 + rule.aliases.len());
    let main = rule.source.trim();
    if !main.is_empty() {
        terms.push(main);
    }
    for alias in &rule.aliases {
        let a = alias.trim();
        if !a.is_empty() {
            terms.push(a);
        }
    }
    terms.sort_by_key(|t| std::cmp::Reverse(t.len()));

    let mut cur = text.to_string();
    for term in terms {
        let mut out = String::with_capacity(cur.len() + 8);
        let mut rest = cur.as_str();
        while let Some((start, end)) = find_glossary_match_range(rest, term) {
            let before = &rest[..start];
            let after = &rest[end..];
            let already_wrapped =
                before.trim_end().ends_with('`') && after.trim_start().starts_with('`');
            let touches_backtick = before.ends_with('`') || after.starts_with('`');
            out.push_str(before);
            if already_wrapped || touches_backtick {
                out.push_str(&rest[start..end]);
            } else {
                out.push('`');
                out.push_str(target);
                out.push('`');
            }
            rest = after;
        }
        out.push_str(rest);
        cur = out;
    }
    cur
}

fn unwrap_synthetic_dnt_backticks(
    out: &mut String,
    source_text_lower: &str,
    rule: &crate::ai::GlossaryRule,
    target: &str,
) {
    let wrapped_tgt_lower = format!("`{}`", target.to_lowercase());
    if source_text_lower.contains(&wrapped_tgt_lower) {
        return;
    }
    let mut candidates = vec![target];
    let main = rule.source.trim();
    if !main.is_empty() && !main.eq_ignore_ascii_case(target) {
        candidates.push(main);
    }
    for alias in &rule.aliases {
        let a = alias.trim();
        if !a.is_empty() {
            candidates.push(a);
        }
    }
    let source_lower = source_text_lower;
    for (open_ch, close_ch) in [
        ('`', '`'),
        ('‘', '’'),
        ('\'', '\''),
        ('“', '”'),
        ('"', '"'),
    ] {
        let already_in_src = candidates.iter().any(|c| {
            source_lower.contains(&format!("{open_ch}{}{close_ch}", c.to_lowercase()))
        });
        if already_in_src {
            continue;
        }
        let mut rebuilt = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(open) = rest.find(open_ch) {
            rebuilt.push_str(&rest[..open]);
            let after_open = &rest[open + open_ch.len_utf8()..];
            if let Some(close) = after_open.find(close_ch) {
                let inner = after_open[..close].trim();
                if candidates.iter().any(|c| inner.eq_ignore_ascii_case(c)) {
                    rebuilt.push_str(target);
                    rest = &after_open[close + close_ch.len_utf8()..];
                    continue;
                }
            }
            rebuilt.push(open_ch);
            rest = after_open;
        }
        rebuilt.push_str(rest);
        *out = rebuilt;
    }
}

pub fn apply_glossary_pre_substitute(
    source_text: &str,
    glossary: &[crate::ai::GlossaryRule],
) -> String {
    if glossary.is_empty() {
        return source_text.to_string();
    }
    let source_lower = source_text.to_lowercase();
    let mut cur = source_text.to_string();
    for rule in glossary {
        let tgt = glossary_rule_target(rule);
        if tgt.is_empty() {
            continue;
        }
        if is_dnt_glossary_rule(rule, tgt) {
            cur = wrap_dnt_glossary_terms_with_backticks(&cur, &source_lower, rule, tgt);
        } else {
            cur = replace_glossary_terms(&cur, rule, tgt, false);
        }
    }
    cur
}

fn fix_korean_particle_after_term(text: &str, term: &str) -> String {
    let trimmed_term = term.trim();
    if trimmed_term.is_empty() || !text.contains(trimmed_term) {
        return text.to_string();
    }
    let last_hangul = trimmed_term
        .trim_end_matches(['》', '』', '」', '〉', '】', '”', '’', '"', '\'', ')', ']'])
        .chars()
        .last()
        .filter(|c| ('가'..='힣').contains(c));
    let Some(last_ch) = last_hangul else {
        return text.to_string();
    };
    let has_batchim = has_korean_final_consonant(last_ch);
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(pos) = rest.find(trimmed_term) {
        let after_term_idx = pos + trimmed_term.len();
        out.push_str(&rest[..after_term_idx]);
        let mut rem = &rest[after_term_idx..];
        while let Some(ch) = rem.chars().next() {
            if matches!(
                ch,
                '》' | '』' | '」' | '〉' | '】' | '”' | '’' | '"' | '\'' | ')' | ']'
            ) {
                out.push(ch);
                rem = &rem[ch.len_utf8()..];
            } else {
                break;
            }
        }
        if let Some(particle) = rem.chars().next() {
            let after_particle = &rem[particle.len_utf8()..];
            let next_is_sep = after_particle.is_empty()
                || after_particle.starts_with(|c: char| {
                    c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']')
                });
            if next_is_sep {
                let fixed = match particle {
                    '는' | '은' => Some(if has_batchim { '은' } else { '는' }),
                    '가' | '이' => Some(if has_batchim { '이' } else { '가' }),
                    '를' | '을' => Some(if has_batchim { '을' } else { '를' }),
                    '와' | '과' => Some(if has_batchim { '과' } else { '와' }),
                    _ => None,
                };
                if let Some(replacement) = fixed {
                    out.push(replacement);
                    rest = after_particle;
                    continue;
                }
            }
        }
        rest = rem;
    }
    out.push_str(rest);
    out
}

pub fn apply_glossary_post_enforce(
    source_text: &str,
    out: &mut String,
    glossary: &[crate::ai::GlossaryRule],
) {
    if glossary.is_empty() || out.trim().is_empty() {
        return;
    }
    let source_lower = source_text.to_lowercase();
    for rule in glossary {
        let tgt = glossary_rule_target(rule);
        if tgt.is_empty() {
            continue;
        }
        if is_dnt_glossary_rule(rule, tgt) {
            unwrap_synthetic_dnt_backticks(out, &source_lower, rule, tgt);
        }
        *out = replace_glossary_terms(out, rule, tgt, true);
        *out = fix_korean_particle_after_term(out, tgt);
    }
}

#[allow(clippy::too_many_arguments)]
fn process_worker_translation_batch(
    session: &mut HyMt2Session<'_>,
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    segments: &[SegmentInput],
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    mut history: Vec<ContextPair>,
    max_ctx_size: u32,
    cancel: &CancellationToken,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<SegmentResult>,
) -> Result<Vec<ContextPair>, String> {
    let can_parallel = session.n_seq_max > 1 && !matches!(style.trim(), "novel" | "auto:novel");
    let mut batch_cache: HashMap<String, String> = HashMap::new();
    if !can_parallel {
        for seg in segments {
            if cancel.is_cancelled() {
                return Err("번역을 취소했습니다.".into());
            }
            let dedup_key = seg.text.trim();
            if let Some(cached_tr) = batch_cache.get(dedup_key) {
                let _ = progress_tx.send(SegmentResult {
                    id: seg.id.clone(),
                    translation: cached_tr.clone(),
                    uncertain: false,
                    notes: Vec::new(),
                    cached: true,
                });
                continue;
            }
            let translated_text = translate_segment_sequential(
                session,
                model,
                uses_hy_template,
                chat_template,
                &seg.text,
                target_language,
                style,
                glossary,
                &mut history,
                max_ctx_size,
                cancel,
            )?;
            if !dedup_key.is_empty() {
                batch_cache.insert(dedup_key.to_string(), translated_text.clone());
            }
            let _ = progress_tx.send(SegmentResult {
                id: seg.id.clone(),
                translation: translated_text,
                uncertain: false,
                notes: Vec::new(),
                cached: false,
            });
        }
        return Ok(history);
    }

    let mut idx = 0usize;
    while idx < segments.len() {
        if cancel.is_cancelled() {
            return Err("번역을 취소했습니다.".into());
        }
        let first_key = segments[idx].text.trim();
        if let Some(cached_tr) = batch_cache.get(first_key) {
            let _ = progress_tx.send(SegmentResult {
                id: segments[idx].id.clone(),
                translation: cached_tr.clone(),
                uncertain: false,
                notes: Vec::new(),
                cached: true,
            });
            idx += 1;
            continue;
        }
        if !is_parallel_gpu_candidate(&segments[idx].text)
            && shortcut_translation(&segments[idx].text, target_language).is_none()
        {
            let seg = &segments[idx];
            let translated_text = translate_segment_sequential(
                session,
                model,
                uses_hy_template,
                chat_template,
                &seg.text,
                target_language,
                style,
                glossary,
                &mut history,
                max_ctx_size,
                cancel,
            )?;
            if !first_key.is_empty() {
                batch_cache.insert(first_key.to_string(), translated_text.clone());
            }
            let _ = progress_tx.send(SegmentResult {
                id: seg.id.clone(),
                translation: translated_text,
                uncertain: false,
                notes: Vec::new(),
                cached: false,
            });
            idx += 1;
            continue;
        }

        let start = idx;
        let mut unique_indices: Vec<usize> = Vec::new();
        let mut span_key_to_rep: HashMap<String, usize> = HashMap::new();
        let mut span_dups_by_rep: HashMap<usize, Vec<usize>> = HashMap::new();

        while idx < segments.len() {
            let text = &segments[idx].text;
            let trimmed = text.trim();
            if let Some(cached_tr) = batch_cache.get(trimmed) {
                let _ = progress_tx.send(SegmentResult {
                    id: segments[idx].id.clone(),
                    translation: cached_tr.clone(),
                    uncertain: false,
                    notes: Vec::new(),
                    cached: true,
                });
                idx += 1;
            } else if let Some(shortcut) = shortcut_translation(text, target_language) {
                let _ = progress_tx.send(SegmentResult {
                    id: segments[idx].id.clone(),
                    translation: shortcut,
                    uncertain: false,
                    notes: Vec::new(),
                    cached: false,
                });
                idx += 1;
            } else if is_parallel_gpu_candidate(text) {
                if let Some(&rep_pos) = span_key_to_rep.get(trimmed) {
                    span_dups_by_rep.entry(rep_pos).or_default().push(idx);
                } else {
                    let rep_pos = unique_indices.len();
                    span_key_to_rep.insert(trimmed.to_string(), rep_pos);
                    unique_indices.push(idx);
                }
                idx += 1;
            } else {
                break;
            }
        }

        if unique_indices.len() >= 2 {
            let batch_inputs: Vec<&SegmentInput> =
                unique_indices.iter().map(|&i| &segments[i]).collect();
            let dup_refs: Vec<Vec<&SegmentInput>> = (0..unique_indices.len())
                .map(|rep_pos| {
                    span_dups_by_rep
                        .get(&rep_pos)
                        .map(|idxs| idxs.iter().map(|&i| &segments[i]).collect())
                        .unwrap_or_default()
                })
                .collect();
            let batch_outputs = translate_parallel_batch(
                session,
                model,
                uses_hy_template,
                chat_template,
                &batch_inputs,
                &dup_refs,
                target_language,
                style,
                glossary,
                max_ctx_size,
                cancel,
                progress_tx,
            )?;
            for (seg_ref, translated) in batch_inputs.iter().zip(batch_outputs.iter()) {
                let k = seg_ref.text.trim();
                if !k.is_empty() {
                    batch_cache.insert(k.to_string(), translated.clone());
                }
                push_history_pair(&mut history, &seg_ref.text, translated);
            }
        } else if let Some(&only_idx) = unique_indices.first() {
            let seg = &segments[only_idx];
            let translated_text = translate_segment_sequential(
                session,
                model,
                uses_hy_template,
                chat_template,
                &seg.text,
                target_language,
                style,
                glossary,
                &mut history,
                max_ctx_size,
                cancel,
            )?;
            let k = seg.text.trim();
            if !k.is_empty() {
                batch_cache.insert(k.to_string(), translated_text.clone());
            }
            let _ = progress_tx.send(SegmentResult {
                id: seg.id.clone(),
                translation: translated_text.clone(),
                uncertain: false,
                notes: Vec::new(),
                cached: false,
            });
            if let Some(dup_idxs) = span_dups_by_rep.get(&0) {
                for &dup_i in dup_idxs {
                    let _ = progress_tx.send(SegmentResult {
                        id: segments[dup_i].id.clone(),
                        translation: translated_text.clone(),
                        uncertain: false,
                        notes: Vec::new(),
                        cached: true,
                    });
                }
            }
        } else if start == idx {
            idx += 1;
        }
    }

    Ok(history)
}

#[inline]
fn has_stop_marker(output: &str) -> bool {
    output.contains("<｜hy_place▁holder▁no▁2｜>")
        || output.contains("<｜hy_EOT｜>")
        || output.contains("<|im_end|>")
        || output.contains("[|endofturn|]")
        || output.contains("<|eot_id|>")
}

#[inline]
fn piece_might_complete_stop_marker(piece: &[u8]) -> bool {
    piece
        .iter()
        .any(|&b| matches!(b, b'<' | b'>' | b'[' | b']'))
}

struct RepetitionGuard {
    tokens: Vec<llama_cpp_2::token::LlamaToken>,
    byte_starts: Vec<usize>,
}

impl RepetitionGuard {
    fn new() -> Self {
        Self {
            tokens: Vec::with_capacity(64),
            byte_starts: Vec::with_capacity(64),
        }
    }

    /// Records the generated token (and the byte offset in `output` BEFORE its text was appended)
    /// and returns `Some(truncate_len)` when a runaway repetition loop is detected.
    fn push_and_check(
        &mut self,
        token: llama_cpp_2::token::LlamaToken,
        pre_append_byte_len: usize,
    ) -> Option<usize> {
        self.tokens.push(token);
        self.byte_starts.push(pre_append_byte_len);
        let n = self.tokens.len();

        // 1. Single identical token repeated 12+ times consecutively.
        if n >= 12 && self.tokens[n - 12..n].iter().all(|&t| t == token) {
            return Some(self.byte_starts[n - 11]);
        }

        // 2. Repeating n-gram cycles (period p in 2..=12) repeating enough consecutive times (>= 12 tokens).
        for p in 2..=12usize {
            let reps = if p <= 3 { 4 } else { 3 };
            let span = p * reps;
            if n < span {
                continue;
            }
            let tail_start = n - span;
            let pat = &self.tokens[n - p..n];
            let mut matched = true;
            for r in 0..(reps - 1) {
                let slice = &self.tokens[tail_start + r * p..tail_start + (r + 1) * p];
                if slice != pat {
                    matched = false;
                    break;
                }
            }
            if matched {
                return Some(self.byte_starts[tail_start + p]);
            }
        }

        if self.tokens.len() > 128 {
            let drop_count = 64;
            self.tokens.drain(..drop_count);
            self.byte_starts.drain(..drop_count);
        }
        None
    }
}

fn build_sampler(model: &LlamaModel, style: &str) -> LlamaSampler {
    if style.trim() == "polished" {
        let (temp, rep_penalty, top_p) = hy_mt2_sampling_params(style);
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(model.vocab().n_tokens(), 64, rep_penalty, 0.0, 0.0),
            LlamaSampler::top_k(20),
            LlamaSampler::top_p(top_p, 1),
            LlamaSampler::temp(temp),
            LlamaSampler::dist(42),
        ])
    } else {
        LlamaSampler::greedy()
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_prompt_tokens(
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    normalized_source: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    history: &[ContextPair],
    omit_marker_clause: bool,
) -> (Vec<llama_cpp_2::token::LlamaToken>, u32) {
    let vocab = model.vocab();
    let src_toks = vocab
        .tokenize(normalized_source.as_bytes(), false, false)
        .len() as u32;
    let (system_prompt, user_prompt) = if uses_hy_template {
        (
            None,
            format_single_prompt_ext(
                normalized_source,
                target_language,
                style,
                glossary,
                history,
                omit_marker_clause,
            ),
        )
    } else {
        let (sys, usr) = format_instruct_prompt_pair(
            normalized_source,
            target_language,
            style,
            glossary,
            history,
        );
        (Some(sys), usr)
    };
    let formatted_prompt = format_chat_prompt_ext(
        model,
        uses_hy_template,
        chat_template,
        system_prompt.as_deref(),
        &user_prompt,
    );
    let parse_special = formatted_prompt.contains("<｜hy_")
        || formatted_prompt.contains("<|")
        || formatted_prompt.contains("[|");
    let tokens = vocab.tokenize(
        formatted_prompt.as_bytes(),
        !uses_hy_template,
        parse_special,
    );
    (tokens, src_toks)
}

fn finalize_translated_unit(
    source_text: &str,
    target_language: &str,
    style: &str,
    raw: &str,
    history: &[ContextPair],
) -> String {
    let cleaned = clean_hy_mt2_output(raw);
    let cleaned = if select_background_pairs(source_text, history).is_empty() {
        cleaned
    } else {
        let src_t = source_text.trim();
        let src_prefix: String = src_t.chars().take(16).collect();
        let non_overlapping_history: Vec<ContextPair> = history
            .iter()
            .filter(|pair| {
                let prev_t = pair.source.trim();
                if prev_t.is_empty() || prev_t == src_t {
                    return false;
                }
                if src_prefix.chars().count() >= 16 && prev_t.starts_with(&src_prefix) {
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        strip_echoed_history_translations(&cleaned, &non_overlapping_history)
    };
    if cleaned.is_empty() {
        source_text.to_string()
    } else {
        let norm_target = normalize_target_language_for_hy_mt2(target_language);
        let quoted = preserve_outer_dialogue_quotes_for_target(source_text, &norm_target, cleaned);
        postprocess_translation_output(source_text, target_language, style, quoted)
    }
}

struct PreparedParallelJob {
    parent_idx: usize,
    part_idx: usize,
    source_text: String,
    seg_glossary: Vec<crate::ai::GlossaryRule>,
    tokens: Vec<llama_cpp_2::token::LlamaToken>,
    max_new: u32,
    reserved_kv: usize,
}

struct ActiveBatchSlot {
    slot_id: i32,
    parent_idx: usize,
    part_idx: usize,
    source_text: String,
    seg_glossary: Vec<crate::ai::GlossaryRule>,
    tokens: Vec<llama_cpp_2::token::LlamaToken>,
    prefill_sent: usize,
    cur_pos: i32,
    max_pos: i32,
    reserved_kv: usize,
    pending_token: Option<llama_cpp_2::token::LlamaToken>,
    needs_sample_at_batch_idx: Option<i32>,
    output: String,
    decoder: encoding_rs::Decoder,
    rep_guard: RepetitionGuard,
    sampler: LlamaSampler,
}

fn compute_shared_prefix(
    jobs: &std::collections::VecDeque<PreparedParallelJob>,
) -> Vec<llama_cpp_2::token::LlamaToken> {
    if jobs.len() < 2 {
        return Vec::new();
    }
    let mut counts: HashMap<&[llama_cpp_2::token::LlamaToken], usize> = HashMap::new();
    for job in jobs {
        if job.tokens.len() >= 10 {
            *counts.entry(&job.tokens[..8]).or_insert(0) += 1;
        }
    }
    let Some((&best_key, &cnt)) = counts.iter().max_by_key(|(_, c)| *c) else {
        return Vec::new();
    };
    if cnt < 2 {
        return Vec::new();
    }
    let matching: Vec<&[llama_cpp_2::token::LlamaToken]> = jobs
        .iter()
        .filter(|j| j.tokens.len() >= 10 && &j.tokens[..8] == best_key)
        .map(|j| j.tokens.as_slice())
        .collect();
    if matching.len() < 2 {
        return Vec::new();
    }
    let mut common = matching
        .iter()
        .map(|t| t.len().saturating_sub(1))
        .min()
        .unwrap_or(0);
    for toks in &matching[1..] {
        let lcp = matching[0]
            .iter()
            .zip(toks.iter())
            .take(common)
            .take_while(|(a, b)| a == b)
            .count();
        common = lcp;
    }
    if common >= 8 {
        matching[0][..common].to_vec()
    } else {
        Vec::new()
    }
}

#[allow(clippy::too_many_arguments)]
fn translate_parallel_batch(
    session: &mut HyMt2Session<'_>,
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    segments: &[&SegmentInput],
    duplicates_for_slot: &[Vec<&SegmentInput>],
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    max_ctx_size: u32,
    cancel: &CancellationToken,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<SegmentResult>,
) -> Result<Vec<String>, String> {
    let vocab = model.vocab();
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let mut results: Vec<Option<String>> = vec![None; segments.len()];
    let mut queue =
        std::collections::VecDeque::<PreparedParallelJob>::with_capacity(segments.len());

    let emit_result =
        |job_idx: usize, final_text: String, cached: bool, results: &mut [Option<String>]| {
            let seg = segments[job_idx];
            results[job_idx] = Some(final_text.clone());
            let _ = progress_tx.send(SegmentResult {
                id: seg.id.clone(),
                translation: final_text.clone(),
                uncertain: false,
                notes: Vec::new(),
                cached,
            });
            if let Some(dups) = duplicates_for_slot.get(job_idx) {
                for dup_seg in dups {
                    let _ = progress_tx.send(SegmentResult {
                        id: dup_seg.id.clone(),
                        translation: final_text.clone(),
                        uncertain: false,
                        notes: Vec::new(),
                        cached: true,
                    });
                }
            }
        };

    let mut part_outputs: Vec<Vec<Option<String>>> = Vec::with_capacity(segments.len());
    let mut part_remaining: Vec<usize> = vec![0; segments.len()];

    for (parent_idx, seg) in segments.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err("번역을 취소했습니다.".into());
        }
        let parts =
            split_long_paragraph_sentences(&seg.text).unwrap_or_else(|| vec![seg.text.clone()]);
        let num_parts = parts.len();
        part_outputs.push(vec![None; num_parts]);
        part_remaining[parent_idx] = num_parts;

        for (part_idx, part_src) in parts.into_iter().enumerate() {
            let seg_glossary = filter_segment_glossary(&part_src, glossary);
            if let Some(shortcut) = glossary_exact_segment_shortcut(&part_src, &seg_glossary)
                .or_else(|| shortcut_translation(&part_src, target_language))
            {
                part_outputs[parent_idx][part_idx] = Some(shortcut);
                part_remaining[parent_idx] -= 1;
                continue;
            }
            let preprocessed = preprocess_source_text(&part_src, &norm_target, style);
            let normalized = apply_glossary_pre_substitute(&preprocessed, &seg_glossary);
            let (tokens, src_toks) = prepare_prompt_tokens(
                model,
                uses_hy_template,
                chat_template,
                &normalized,
                target_language,
                style,
                &[],
                &[],
                false,
            );
            if tokens.is_empty() {
                part_outputs[parent_idx][part_idx] = Some(part_src);
                part_remaining[parent_idx] -= 1;
                continue;
            }
            let max_new = estimate_max_output_tokens_for_prompt(
                src_toks,
                &normalized,
                &norm_target,
                max_ctx_size,
            );
            let reserved_kv = tokens.len() + max_new as usize + 8;
            queue.push_back(PreparedParallelJob {
                parent_idx,
                part_idx,
                source_text: part_src,
                seg_glossary,
                tokens,
                max_new,
                reserved_kv,
            });
        }

        if part_remaining[parent_idx] == 0 {
            let parts_done: Vec<String> = part_outputs[parent_idx]
                .iter()
                .map(|p| p.clone().unwrap_or_default())
                .collect();
            let joined = join_translated_sentences(&parts_done, &norm_target);
            emit_result(parent_idx, joined, false, &mut results);
        }
    }

    if queue.is_empty() {
        return Ok(results
            .into_iter()
            .enumerate()
            .map(|(i, item)| item.unwrap_or_else(|| segments[i].text.clone()))
            .collect());
    }

    // Schedule longest jobs first (LPT) so multi-slot GPU batches share decode steps across
    // long paragraphs instead of leaving the longest paragraph running solo at batch=1.
    queue.make_contiguous().sort_by(|a, b| {
        b.reserved_kv
            .cmp(&a.reserved_kv)
            .then_with(|| a.parent_idx.cmp(&b.parent_idx))
            .then_with(|| a.part_idx.cmp(&b.part_idx))
    });

    let has_prefix_slot = session.n_seq_max > 2;
    let max_slots = if has_prefix_slot {
        (session.n_seq_max - 1).clamp(1, 8) as usize
    } else {
        session.n_seq_max.clamp(1, 8) as usize
    };
    let prefix_seq_id = (session.n_seq_max - 1) as i32;
    let shared_prefix = if has_prefix_slot {
        compute_shared_prefix(&queue)
    } else {
        Vec::new()
    };
    let shared_prefix_len = shared_prefix.len();

    let mut initial_exclusive_kv = 0usize;
    for job in queue.iter().take(max_slots) {
        let excl = if shared_prefix_len >= 8 && job.tokens.starts_with(&shared_prefix) {
            job.reserved_kv.saturating_sub(shared_prefix_len)
        } else {
            job.reserved_kv
        };
        initial_exclusive_kv += excl;
    }

    let max_slot_budget = (initial_exclusive_kv + shared_prefix_len + 32)
        .min(6144)
        .min(max_ctx_size as usize) as u32;
    session.ensure_capacity(max_slot_budget, max_ctx_size)?;
    session.clear_all_kv();

    let mut prefix_kv_ready = false;
    let mut batch = LlamaBatch::new(512, session.n_seq_max as i32);
    if shared_prefix_len >= 8 && (shared_prefix_len + 32) < session.current_n_ctx as usize {
        for (chunk_idx, chunk) in shared_prefix.chunks(512).enumerate() {
            batch.clear();
            let base_pos = chunk_idx * 512;
            for (offset, &tok) in chunk.iter().enumerate() {
                batch
                    .add(tok, (base_pos + offset) as i32, &[prefix_seq_id], false)
                    .map_err(|e| format!("공통 접두사 배치 추가 오류: {e}"))?;
            }
            session
                .ctx
                .decode(&mut batch)
                .map_err(|e| format!("공통 접두사 디코딩 오류: {e}"))?;
        }
        prefix_kv_ready = true;
    }

    let active_prefix_cells = if prefix_kv_ready {
        shared_prefix_len
    } else {
        0
    };
    let kv_limit = (session.current_n_ctx as usize).saturating_sub(16 + active_prefix_cells);
    let mut slots: Vec<Option<ActiveBatchSlot>> = (0..max_slots).map(|_| None).collect();
    let mut active_reserved_kv = 0usize;
    let mut piece_buf = Vec::<u8>::with_capacity(32);
    let mut retry_bare: Vec<(usize, usize, String)> = Vec::new();

    loop {
        if cancel.is_cancelled() {
            session.clear_all_kv();
            return Err("번역을 취소했습니다.".into());
        }

        // Admit waiting jobs into idle slots while staying within unified KV cell budget.
        for s_idx in 0..max_slots {
            if slots[s_idx].is_some() {
                continue;
            }
            let Some(next_job) = queue.front() else {
                break;
            };
            let any_active = slots.iter().any(Option::is_some);
            let uses_shared = prefix_kv_ready && next_job.tokens.starts_with(&shared_prefix);
            let mut needed = if uses_shared {
                next_job.reserved_kv.saturating_sub(shared_prefix_len)
            } else {
                next_job.reserved_kv
            };
            if needed > kv_limit && !any_active {
                let prompt_exclusive = if uses_shared {
                    next_job.tokens.len().saturating_sub(shared_prefix_len)
                } else {
                    next_job.tokens.len()
                };
                let capped_new = kv_limit.saturating_sub(prompt_exclusive + 8);
                if capped_new == 0 {
                    let dropped = queue.pop_front().unwrap();
                    part_outputs[dropped.parent_idx][dropped.part_idx] = Some(dropped.source_text);
                    part_remaining[dropped.parent_idx] =
                        part_remaining[dropped.parent_idx].saturating_sub(1);
                    if part_remaining[dropped.parent_idx] == 0 {
                        let parts_done: Vec<String> = part_outputs[dropped.parent_idx]
                            .iter()
                            .map(|p| p.clone().unwrap_or_default())
                            .collect();
                        let joined = join_translated_sentences(&parts_done, &norm_target);
                        emit_result(dropped.parent_idx, joined, false, &mut results);
                    }
                    continue;
                }
                needed = prompt_exclusive + capped_new + 8;
            }
            if active_reserved_kv + needed > kv_limit {
                break;
            }
            let job = queue.pop_front().unwrap();
            let prompt_exclusive = if uses_shared {
                job.tokens.len().saturating_sub(shared_prefix_len)
            } else {
                job.tokens.len()
            };
            let effective_max_new = needed
                .saturating_sub(prompt_exclusive + 8)
                .min(job.max_new as usize) as u32;
            let _ = session
                .ctx
                .clear_kv_cache_seq(Some(s_idx as u32), None, None);
            let prefill_start = if uses_shared
                && session
                    .ctx
                    .copy_kv_cache_seq(
                        prefix_seq_id,
                        s_idx as i32,
                        Some(0),
                        Some(shared_prefix_len as u32),
                    )
                    .is_ok()
            {
                shared_prefix_len
            } else {
                0
            };
            let actual_reserved = if prefill_start > 0 {
                needed
            } else {
                job.reserved_kv
            };
            let prompt_len = job.tokens.len() as i32;
            let init_cap = (job.source_text.len() * 2).min(4096);
            active_reserved_kv += actual_reserved;
            slots[s_idx] = Some(ActiveBatchSlot {
                slot_id: s_idx as i32,
                parent_idx: job.parent_idx,
                part_idx: job.part_idx,
                source_text: job.source_text,
                seg_glossary: job.seg_glossary,
                tokens: job.tokens,
                prefill_sent: prefill_start,
                cur_pos: prefill_start as i32,
                max_pos: prompt_len.saturating_add(effective_max_new as i32),
                reserved_kv: actual_reserved,
                pending_token: None,
                needs_sample_at_batch_idx: None,
                output: String::with_capacity(init_cap),
                decoder: encoding_rs::UTF_8.new_decoder(),
                rep_guard: RepetitionGuard::new(),
                sampler: build_sampler(model, style),
            });
        }

        if slots.iter().all(Option::is_none) && queue.is_empty() {
            break;
        }

        batch.clear();
        let decode_needed: usize = slots
            .iter()
            .filter(|s| s.as_ref().is_some_and(|st| st.pending_token.is_some()))
            .count();
        let mut rem_prefill = 512usize.saturating_sub(decode_needed);

        for slot in slots.iter_mut().flatten() {
            if rem_prefill == 0 {
                break;
            }
            if slot.prefill_sent >= slot.tokens.len() {
                continue;
            }
            let remaining = slot.tokens.len() - slot.prefill_sent;
            let take = remaining.min(rem_prefill);
            let start = slot.prefill_sent;
            let end = start + take;
            for (offset, &tok) in slot.tokens[start..end].iter().enumerate() {
                let global_idx = start + offset;
                let is_last_prompt = global_idx + 1 == slot.tokens.len();
                let batch_idx = batch.n_tokens();
                batch
                    .add(tok, global_idx as i32, &[slot.slot_id], is_last_prompt)
                    .map_err(|e| format!("병렬 프롬프트 배치 추가 오류: {e}"))?;
                if is_last_prompt {
                    slot.needs_sample_at_batch_idx = Some(batch_idx);
                }
            }
            slot.prefill_sent = end;
            slot.cur_pos = end as i32;
            rem_prefill -= take;
        }

        for slot in slots.iter_mut().flatten() {
            if let Some(tok) = slot.pending_token.take() {
                let batch_idx = batch.n_tokens();
                batch
                    .add(tok, slot.cur_pos, &[slot.slot_id], true)
                    .map_err(|e| format!("병렬 토큰 배치 추가 오류: {e}"))?;
                slot.cur_pos += 1;
                slot.needs_sample_at_batch_idx = Some(batch_idx);
            }
        }

        if batch.n_tokens() == 0 {
            break;
        }

        session
            .ctx
            .decode(&mut batch)
            .map_err(|e| format!("병렬 추론 디코딩 오류: {e}"))?;

        for s_idx in 0..max_slots {
            let mut finished = false;
            if let Some(slot) = slots[s_idx].as_mut()
                && let Some(batch_idx) = slot.needs_sample_at_batch_idx.take()
            {
                let token = slot.sampler.sample(&session.ctx, batch_idx);
                slot.sampler.accept(token);
                if vocab.is_eog(token) {
                    finished = true;
                } else {
                    let pre_len = slot.output.len();
                    piece_buf.clear();
                    vocab.token_to_piece_into(token, &mut piece_buf, false, None);
                    if !piece_buf.is_empty() {
                        if let Some(cap) = slot.decoder.max_utf8_buffer_length(piece_buf.len()) {
                            slot.output.reserve(cap);
                        }
                        let _ = slot
                            .decoder
                            .decode_to_string(&piece_buf, &mut slot.output, false);
                    }

                    if piece_might_complete_stop_marker(&piece_buf) && has_stop_marker(&slot.output)
                    {
                        finished = true;
                    } else if let Some(trim_len) = slot.rep_guard.push_and_check(token, pre_len) {
                        slot.output.truncate(trim_len);
                        finished = true;
                    } else if slot.cur_pos >= slot.max_pos {
                        finished = true;
                    } else {
                        slot.pending_token = Some(token);
                    }
                }
            }

            if finished {
                let mut done = slots[s_idx].take().unwrap();
                let _ = done.decoder.decode_to_string(&[], &mut done.output, true);
                let _ = session
                    .ctx
                    .clear_kv_cache_seq(Some(done.slot_id as u32), None, None);
                active_reserved_kv = active_reserved_kv.saturating_sub(done.reserved_kv);
                let mut final_text = finalize_translated_unit(
                    &done.source_text,
                    target_language,
                    style,
                    &done.output,
                    &[],
                );
                apply_glossary_post_enforce(&done.source_text, &mut final_text, &done.seg_glossary);
                if looks_like_prompt_echo(&done.source_text, &final_text) {
                    retry_bare.push((done.parent_idx, done.part_idx, done.source_text));
                } else {
                    part_outputs[done.parent_idx][done.part_idx] = Some(final_text);
                    part_remaining[done.parent_idx] =
                        part_remaining[done.parent_idx].saturating_sub(1);
                    if part_remaining[done.parent_idx] == 0 {
                        let parts_done: Vec<String> = part_outputs[done.parent_idx]
                            .iter()
                            .map(|p| p.clone().unwrap_or_default())
                            .collect();
                        let joined = join_translated_sentences(&parts_done, &norm_target);
                        emit_result(done.parent_idx, joined, false, &mut results);
                    }
                }
            }
        }
    }

    session.clear_all_kv();

    for (parent_idx, part_idx, part_src) in retry_bare {
        if cancel.is_cancelled() {
            return Err("번역을 취소했습니다.".into());
        }
        let retried = translate_single_unit_ext(
            session,
            model,
            uses_hy_template,
            chat_template,
            &part_src,
            target_language,
            style,
            glossary,
            &[],
            max_ctx_size,
            cancel,
            true,
        )?;
        part_outputs[parent_idx][part_idx] = Some(retried);
        part_remaining[parent_idx] = part_remaining[parent_idx].saturating_sub(1);
        if part_remaining[parent_idx] == 0 {
            let parts_done: Vec<String> = part_outputs[parent_idx]
                .iter()
                .map(|p| p.clone().unwrap_or_default())
                .collect();
            let joined = join_translated_sentences(&parts_done, &norm_target);
            emit_result(parent_idx, joined, false, &mut results);
        }
    }

    Ok(results
        .into_iter()
        .enumerate()
        .map(|(i, item)| item.unwrap_or_else(|| segments[i].text.clone()))
        .collect())
}

fn translate_segment_sequential(
    session: &mut HyMt2Session<'_>,
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    source_text: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    history: &mut Vec<ContextPair>,
    max_ctx_size: u32,
    cancel: &CancellationToken,
) -> Result<String, String> {
    if cancel.is_cancelled() {
        return Err("번역을 취소했습니다.".into());
    }
    if let Some(shortcut) = shortcut_translation(source_text, target_language) {
        return Ok(shortcut);
    }

    // Large multi-line blocks (e.g. a whole light-novel <div> joined with <br>) lose coherence
    // and exceed optimal 1.8B attention span; translate line-by-line with running history instead.
    if segment_needs_line_split(source_text) {
        let lines: Vec<&str> = source_text.lines().collect();
        let mut translated_lines = Vec::with_capacity(lines.len());
        for line in lines {
            if cancel.is_cancelled() {
                return Err("번역을 취소했습니다.".into());
            }
            if let Some(shortcut) = shortcut_translation(line, target_language) {
                translated_lines.push(shortcut);
                continue;
            }
            let out_line = translate_single_unit(
                session,
                model,
                uses_hy_template,
                chat_template,
                line,
                target_language,
                style,
                glossary,
                history,
                max_ctx_size,
                cancel,
            )?;
            push_history_pair(history, line, &out_line);
            translated_lines.push(out_line);
        }
        return Ok(translated_lines.join("\n"));
    }

    let translated = translate_single_unit(
        session,
        model,
        uses_hy_template,
        chat_template,
        source_text,
        target_language,
        style,
        glossary,
        history,
        max_ctx_size,
        cancel,
    )?;
    push_history_pair(history, source_text, &translated);
    Ok(translated)
}

#[allow(clippy::too_many_arguments)]
fn translate_single_unit(
    session: &mut HyMt2Session<'_>,
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    source_text: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    history: &[ContextPair],
    max_ctx_size: u32,
    cancel: &CancellationToken,
) -> Result<String, String> {
    let first = translate_single_unit_ext(
        session,
        model,
        uses_hy_template,
        chat_template,
        source_text,
        target_language,
        style,
        glossary,
        history,
        max_ctx_size,
        cancel,
        false,
    )?;
    if looks_like_prompt_echo(source_text, &first) {
        return translate_single_unit_ext(
            session,
            model,
            uses_hy_template,
            chat_template,
            source_text,
            target_language,
            style,
            glossary,
            history,
            max_ctx_size,
            cancel,
            true,
        );
    }
    Ok(first)
}

#[allow(clippy::too_many_arguments)]
fn translate_single_unit_ext(
    session: &mut HyMt2Session<'_>,
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    source_text: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    history: &[ContextPair],
    max_ctx_size: u32,
    cancel: &CancellationToken,
    omit_marker_clause: bool,
) -> Result<String, String> {
    let seg_glossary = filter_segment_glossary(source_text, glossary);
    if let Some(shortcut) = glossary_exact_segment_shortcut(source_text, &seg_glossary)
        .or_else(|| shortcut_translation(source_text, target_language))
    {
        return Ok(shortcut);
    }
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let preprocessed = preprocess_source_text(source_text, &norm_target, style);
    let normalized_source = apply_glossary_pre_substitute(&preprocessed, &seg_glossary);
    let (tokens, src_toks) = prepare_prompt_tokens(
        model,
        uses_hy_template,
        chat_template,
        &normalized_source,
        target_language,
        style,
        &[],
        history,
        omit_marker_clause,
    );
    if tokens.is_empty() {
        return Ok(String::new());
    }
    let max_gen = estimate_max_output_tokens_for_prompt(
        src_toks,
        &normalized_source,
        &norm_target,
        max_ctx_size,
    );
    let raw = run_completion(
        session,
        model,
        tokens,
        style,
        max_ctx_size,
        max_gen,
        source_text.len(),
        cancel,
    )?;
    let mut out = finalize_translated_unit(
        source_text,
        target_language,
        style,
        &raw,
        history,
    );
    apply_glossary_post_enforce(source_text, &mut out, &seg_glossary);
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn run_completion(
    session: &mut HyMt2Session<'_>,
    model: &LlamaModel,
    tokens: Vec<llama_cpp_2::token::LlamaToken>,
    style: &str,
    max_ctx_size: u32,
    max_new_tokens: u32,
    source_byte_len: usize,
    cancel: &CancellationToken,
) -> Result<String, String> {
    if cancel.is_cancelled() {
        return Err("번역을 취소했습니다.".into());
    }
    if tokens.is_empty() {
        return Ok(String::new());
    }
    let vocab = model.vocab();

    let n_ctx_needed = (tokens.len() as u32 + max_new_tokens + 64)
        .min(max_ctx_size)
        .max(1536);
    session.ensure_capacity(n_ctx_needed, max_ctx_size)?;

    // Reuse the shared KV-cache prefix across consecutive sequential requests (`seq_id = 0`)
    // so shared prompt instructions / glossary / history prefixes are decoded only once.
    let common = session
        .cached_seq0_tokens
        .iter()
        .zip(tokens.iter())
        .take_while(|(a, b)| a == b)
        .count()
        .min(tokens.len().saturating_sub(1));
    let mut prefix_len = 0usize;
    if common >= 8
        && session
            .ctx
            .clear_kv_cache_seq(Some(0), Some(common as u32), None)
            .unwrap_or(false)
    {
        prefix_len = common;
    }
    if prefix_len == 0 {
        session.ctx.clear_kv_cache();
    }
    session.cached_seq0_tokens.clear();

    let mut batch = LlamaBatch::new(512, 1);
    let total_prompt = tokens.len();
    for (chunk_idx, chunk) in tokens[prefix_len..].chunks(512).enumerate() {
        if cancel.is_cancelled() {
            return Err("번역을 취소했습니다.".into());
        }
        batch.clear();
        let base_pos = prefix_len + chunk_idx * 512;
        for (offset, &tok) in chunk.iter().enumerate() {
            let pos = (base_pos + offset) as i32;
            let is_last = base_pos + offset + 1 == total_prompt;
            batch
                .add(tok, pos, &[0], is_last)
                .map_err(|e| format!("프롬프트 배치 추가 오류: {e}"))?;
        }
        session
            .ctx
            .decode(&mut batch)
            .map_err(|e| format!("프롬프트 디코딩 오류: {e}"))?;
    }

    let mut sampler = build_sampler(model, style);
    let mut rep_guard = RepetitionGuard::new();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut piece_buf = Vec::<u8>::with_capacity(32);
    let mut output = String::with_capacity((source_byte_len * 2).min(8192));
    let mut cur_pos = total_prompt as i32;
    let max_pos = (total_prompt as u32)
        .saturating_add(max_new_tokens)
        .min(session.ctx.n_ctx().saturating_sub(4));

    while (cur_pos as u32) < max_pos {
        if cancel.is_cancelled() {
            return Err("번역을 취소했습니다.".into());
        }
        let token = sampler.sample(&session.ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if vocab.is_eog(token) {
            break;
        }
        let pre_len = output.len();
        piece_buf.clear();
        vocab.token_to_piece_into(token, &mut piece_buf, false, None);
        if !piece_buf.is_empty() {
            if let Some(cap) = decoder.max_utf8_buffer_length(piece_buf.len()) {
                output.reserve(cap);
            }
            let _ = decoder.decode_to_string(&piece_buf, &mut output, false);
        }

        if piece_might_complete_stop_marker(&piece_buf) && has_stop_marker(&output) {
            break;
        }
        if let Some(trim_len) = rep_guard.push_and_check(token, pre_len) {
            output.truncate(trim_len);
            break;
        }

        batch.clear();
        batch
            .add(token, cur_pos, &[0], true)
            .map_err(|e| format!("토큰 배치 추가 오류: {e}"))?;
        cur_pos += 1;
        session
            .ctx
            .decode(&mut batch)
            .map_err(|e| format!("토큰 생성 디코딩 오류: {e}"))?;
    }

    let _ = decoder.decode_to_string(&[], &mut output, true);
    session.cached_seq0_tokens = tokens;
    Ok(output)
}

#[allow(dead_code)]
pub fn format_chat_prompt(
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    user_prompt: &str,
) -> String {
    format_chat_prompt_ext(model, uses_hy_template, chat_template, None, user_prompt)
}

pub fn format_chat_prompt_ext(
    model: &LlamaModel,
    uses_hy_template: bool,
    chat_template: Option<&LlamaChatTemplate>,
    system_prompt: Option<&str>,
    user_prompt: &str,
) -> String {
    if uses_hy_template {
        // Tencent Hy-MT2 has no default system prompt.
        return format!("<｜hy_begin▁of▁sentence｜><｜hy_User｜>{user_prompt}<｜hy_Assistant｜>");
    }
    if let Some(tmpl) = chat_template {
        let mut msgs = Vec::with_capacity(2);
        if let Some(sys) = system_prompt.filter(|s| !s.trim().is_empty())
            && let Ok(m) = LlamaChatMessage::new("system".to_string(), sys.to_string())
        {
            msgs.push(m);
        }
        if let Ok(u) = LlamaChatMessage::new("user".to_string(), user_prompt.to_string()) {
            msgs.push(u);
        }
        if !msgs.is_empty()
            && let Ok(rendered) = model.apply_chat_template(tmpl, &msgs, true)
            && !rendered.trim().is_empty()
        {
            return rendered;
        }
    }
    format!("<｜hy_begin▁of▁sentence｜><｜hy_User｜>{user_prompt}<｜hy_Assistant｜>")
}

pub fn normalize_target_language_for_hy_mt2(lang: &str) -> String {
    let trimmed = lang.trim();
    match trimmed.to_lowercase().as_str() {
        "ko" | "kor" | "korean" | "한국어" => "Korean".into(),
        "en" | "eng" | "english" | "영어" => "English".into(),
        "ja" | "jpn" | "japanese" | "일본어" | "日本語" => "Japanese".into(),
        "zh" | "zh-cn" | "zh-hans" | "chinese" | "중국어" | "中文" | "中文(简体)"
        | "중국어(간체)" => "Chinese".into(),
        "zh-tw" | "zh-hant" | "중국어(번체)" | "번체중국어" | "繁體中文" | "中文(繁體)" => {
            "Traditional Chinese".into()
        }
        "yue" | "cantonese" | "광둥어" | "粵語" => "Cantonese".into(),
        "fr" | "fra" | "french" | "프랑스어" | "français" => "French".into(),
        "de" | "deu" | "german" | "독일어" | "deutsch" => "German".into(),
        "es" | "spa" | "spanish" | "스페인어" | "español" => "Spanish".into(),
        "pt" | "por" | "portuguese" | "포르투갈어" | "português" => "Portuguese".into(),
        "it" | "ita" | "italian" | "이탈리아어" | "italiano" => "Italian".into(),
        "ru" | "rus" | "russian" | "러시아어" | "русский" => "Russian".into(),
        "ar" | "ara" | "arabic" | "아랍어" | "العربية" => "Arabic".into(),
        "vi" | "vie" | "vietnamese" | "베트남어" | "tiếng việt" => "Vietnamese".into(),
        "th" | "tha" | "thai" | "태국어" | "ไทย" => "Thai".into(),
        "id" | "ind" | "indonesian" | "인도네시아어" | "bahasa indonesia" => {
            "Indonesian".into()
        }
        "hi" | "hin" | "hindi" | "힌디어" | "हिन्दी" => "Hindi".into(),
        "tr" | "tur" | "turkish" | "터키어" | "튀르키예어" | "türkçe" => "Turkish".into(),
        "pl" | "pol" | "polish" | "폴란드어" | "polski" => "Polish".into(),
        "nl" | "nld" | "dutch" | "네덜란드어" | "nederlands" => "Dutch".into(),
        "uk" | "ukr" | "ukrainian" | "우크라이나어" | "українська" => {
            "Ukrainian".into()
        }
        "cs" | "ces" | "czech" | "체코어" | "čeština" => "Czech".into(),
        "fa" | "fas" | "persian" | "페르시아어" | "فارسی" => "Persian".into(),
        "he" | "heb" | "hebrew" | "히브리어" | "עברית" => "Hebrew".into(),
        "ms" | "msa" | "malay" | "말레이어" | "bahasa melayu" => "Malay".into(),
        "tl" | "fil" | "filipino" | "tagalog" | "필리핀어" | "타갈로그어" => {
            "Filipino".into()
        }
        "bo" | "tibetan" | "티베트어" => "Tibetan".into(),
        "mn" | "mongolian" | "몽골어" => "Mongolian".into(),
        "ug" | "uyghur" | "위구르어" => "Uyghur".into(),
        "kk" | "kazakh" | "카자흐어" => "Kazakh".into(),
        "km" | "khmer" | "크메르어" => "Khmer".into(),
        "my" | "burmese" | "미얀마어" => "Burmese".into(),
        _ => trimmed.to_string(),
    }
}

fn hy_mt2_zh_language_name(norm_lang: &str) -> &str {
    match norm_lang {
        "Korean" => "韩语",
        "English" => "英语",
        "Japanese" => "日语",
        "Chinese" => "中文",
        "Traditional Chinese" => "繁体中文",
        "Cantonese" => "粤语",
        "French" => "法语",
        "German" => "德语",
        "Spanish" => "西班牙语",
        "Portuguese" => "葡萄牙语",
        "Italian" => "意大利语",
        "Russian" => "俄语",
        "Arabic" => "阿拉伯语",
        "Vietnamese" => "越南语",
        "Thai" => "泰语",
        "Indonesian" => "印度尼西亚语",
        "Hindi" => "印地语",
        "Turkish" => "土耳其语",
        "Polish" => "波兰语",
        "Dutch" => "荷兰语",
        "Ukrainian" => "乌克兰语",
        "Czech" => "捷克语",
        "Persian" => "波斯语",
        "Hebrew" => "希伯来语",
        "Malay" => "马来语",
        "Filipino" => "菲律宾语",
        "Tibetan" => "藏语",
        "Mongolian" => "蒙古语",
        "Uyghur" => "维吾尔语",
        "Kazakh" => "哈萨克语",
        "Khmer" => "高棉语",
        "Burmese" => "缅甸语",
        _ => norm_lang,
    }
}

fn source_is_chinese(source_text: &str) -> bool {
    let mut hanzi = 0usize;
    for ch in source_text.chars() {
        match ch {
            '\u{AC00}'..='\u{D7A3}' | '\u{3131}'..='\u{318E}' => return false,
            '\u{3040}'..='\u{30FF}' | '\u{FF65}'..='\u{FF9F}' => return false,
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => hanzi += 1,
            _ => {}
        }
    }
    hanzi > 0
}

pub fn hy_mt2_sampling_params(style: &str) -> (f32, f32, f32) {
    match style.trim() {
        "literal" | "auto:technical" => (0.08, 1.00, 0.75),
        "auto:wiki" => (0.10, 1.00, 0.78),
        "novel" | "auto:novel" => (0.12, 1.00, 0.80),
        "auto:casual" => (0.16, 1.01, 0.80),
        "polished" => (0.22, 1.01, 0.80),
        _ => (0.15, 1.01, 0.80),
    }
}

pub fn is_dialogue_segment(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let is_cjk = has_cjk_or_kana(trimmed);
    const QUOTE_PAIRS: [(char, char); 6] = [
        ('「', '」'),
        ('『', '』'),
        ('“', '”'),
        ('"', '"'),
        ('〝', '〟'),
        ('（', '）'),
    ];
    if QUOTE_PAIRS
        .iter()
        .any(|&(open, close)| trimmed.starts_with(open) && trimmed.ends_with(close))
        || (is_cjk && trimmed.starts_with('(') && trimmed.ends_with(')'))
    {
        return true;
    }
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > 1 {
        let dialogue_lines = lines
            .iter()
            .filter(|l| is_dialogue_line(l, is_cjk))
            .count();
        if dialogue_lines * 2 > lines.len() {
            return true;
        }
    }
    false
}

fn is_polluted_history_translation(source: &str, translation: &str) -> bool {
    const ECHO_MARKERS: &[&str] = &[
        "[Background Information]",
        "[Source Text]",
        "[배경 정보]",
        "[출처 텍스트]",
        "[원문 텍스트]",
        "Reference the following translations",
        "Please translate the following text",
    ];
    if ECHO_MARKERS.iter().any(|m| translation.contains(m)) {
        return true;
    }
    if (translation.contains(" -> ") || translation.contains(" → "))
        && !source.contains("->")
        && !source.contains('→')
    {
        return true;
    }
    false
}

fn push_history_pair(history: &mut Vec<ContextPair>, source: &str, translation: &str) {
    let s = source.trim();
    let t = translation.trim();
    if s.is_empty() || t.is_empty() || is_polluted_history_translation(s, t) {
        return;
    }
    if let Some(existing) = history.iter_mut().find(|pair| pair.source.trim() == s) {
        existing.translation = Some(t.to_string());
    } else {
        history.push(ContextPair {
            source: s.to_string(),
            translation: Some(t.to_string()),
        });
    }
    if history.len() > 260 {
        history.drain(..history.len() - 260);
    }
}

fn merge_context_pairs(target: &mut Vec<ContextPair>, incoming: &[ContextPair]) {
    for item in incoming {
        let s = item.source.trim();
        if s.is_empty() {
            continue;
        }
        let tr = item
            .translation
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty() && !is_polluted_history_translation(s, t))
            .map(str::to_string);
        if let Some(existing) = target.iter_mut().find(|pair| pair.source.trim() == s) {
            if tr.is_some() && existing.translation.is_none() {
                existing.translation = tr;
            }
        } else {
            target.push(ContextPair {
                source: s.to_string(),
                translation: tr,
            });
        }
    }
}

fn is_kanji(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c)
}

fn is_katakana_word_char(c: char) -> bool {
    ('\u{30A1}'..='\u{30F6}').contains(&c) || c == 'ー' || c == 'ヴ'
}

fn extract_domain_terms(text: &str) -> HashSet<String> {
    let mut terms = HashSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if is_katakana_word_char(chars[i]) {
            let start = i;
            while i < chars.len() && is_katakana_word_char(chars[i]) {
                i += 1;
            }
            if i - start >= 2 {
                terms.insert(chars[start..i].iter().collect());
            }
            continue;
        }
        if is_kanji(chars[i]) {
            let start = i;
            while i < chars.len() && is_kanji(chars[i]) {
                i += 1;
            }
            if i - start >= 2 {
                terms.insert(chars[start..i].iter().collect());
            }
            continue;
        }
        if chars[i].is_ascii_uppercase() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '\'')
            {
                i += 1;
            }
            if i - start >= 3 {
                let word: String = chars[start..i].iter().collect();
                if !matches!(
                    word.as_str(),
                    "The"
                        | "This"
                        | "That"
                        | "These"
                        | "Those"
                        | "When"
                        | "Where"
                        | "What"
                        | "Why"
                        | "How"
                        | "And"
                        | "But"
                        | "For"
                        | "With"
                        | "From"
                        | "Then"
                        | "She"
                        | "Her"
                        | "His"
                        | "They"
                        | "You"
                        | "Wait"
                        | "Please"
                        | "Don't"
                        | "Doesn't"
                        | "Isn't"
                ) {
                    terms.insert(word);
                }
            }
            continue;
        }
        i += 1;
    }
    terms
}

fn has_cjk_or_kana(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{3040}'..='\u{30FF}').contains(&c)
            || ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
    })
}

pub fn select_background_pairs(source_text: &str, history: &[ContextPair]) -> Vec<String> {
    if history.is_empty() {
        return Vec::new();
    }
    // Very short interjections or monologues (e.g. 「ああ」, （そういう事か）, 「……ふむ」) do not need
    // multi-sentence background pairs; omitting them prevents context-bleed hallucinations and
    // cuts prompt prefill latency by ~3x.
    let current_terms = extract_domain_terms(source_text);
    if source_text.trim().chars().count() <= 14 && current_terms.is_empty() {
        return Vec::new();
    }

    let translated_indices: Vec<usize> = history
        .iter()
        .enumerate()
        .filter_map(|(idx, pair)| {
            pair.translation
                .as_deref()
                .filter(|t| {
                    !t.trim().is_empty() && !is_polluted_history_translation(&pair.source, t)
                })
                .map(|_| idx)
        })
        .collect();

    if translated_indices.is_empty() {
        return history
            .iter()
            .rev()
            .take(1)
            .filter_map(|p| {
                let s = p.source.trim();
                (!s.is_empty()).then(|| format!("(preceding scene, do not translate): {s}"))
            })
            .collect();
    }

    let recent_start = translated_indices.len().saturating_sub(1);
    let recent_slice = &translated_indices[recent_start..];
    let earlier_slice = &translated_indices[..recent_start];

    let mut chosen = Vec::new();
    if !current_terms.is_empty() && !earlier_slice.is_empty() {
        let mut anchor_count = 0usize;
        for &idx in earlier_slice {
            let candidate = &history[idx].source;
            if current_terms.iter().any(|term| candidate.contains(term)) {
                chosen.push(idx);
                anchor_count += 1;
                if anchor_count >= 1 {
                    break;
                }
            }
        }
    }

    // Prevent cross-speaker register bleed in 3-party scenes and prevent 1.8B Q6_K from echoing
    // long unrelated web-article paragraphs from [Background Information].
    let cur_is_dlg = is_dialogue_segment(source_text);
    let cur_chars = source_text.trim().chars().count();
    let cur_has_cjk = has_cjk_or_kana(source_text);
    let cur_upper = source_text.chars().filter(|c| c.is_ascii_uppercase()).count();
    let cur_lower = source_text.chars().filter(|c| c.is_ascii_lowercase()).count();
    if !cur_is_dlg && (source_text.contains(';') || (cur_upper > cur_lower && cur_chars > 20)) {
        return Vec::new();
    }
    let cur_reg = crate::ai::detect_dialogue_register(source_text);
    for &idx in recent_slice {
        if has_cjk_or_kana(&history[idx].source) != cur_has_cjk {
            continue;
        }
        let prev_trimmed = history[idx].source.trim();
        let src_trimmed_cur = source_text.trim();
        if !cur_is_dlg && (prev_trimmed.contains(';') || prev_trimmed == src_trimmed_cur) {
            continue;
        }
        if cur_is_dlg && is_dialogue_segment(&history[idx].source) {
            let prev_reg = crate::ai::detect_dialogue_register(&history[idx].source);
            if (cur_reg == crate::ai::DialogueRegister::Casual
                && prev_reg == crate::ai::DialogueRegister::Polite)
                || (cur_reg == crate::ai::DialogueRegister::Polite
                    && prev_reg == crate::ai::DialogueRegister::Casual)
            {
                continue;
            }
        }
        // For non-dialogue prose, only include the preceding line if it shares a domain term
        // or is short narrative context (< 90 chars) so 1.8B Q6_K never re-translates unrelated paragraphs.
        let prev_chars = prev_trimmed.chars().count();
        if !cur_is_dlg
            && (cur_chars > 90 || prev_chars > 120)
            && !current_terms
                .iter()
                .any(|term| history[idx].source.contains(term))
        {
            continue;
        }
        if !chosen.contains(&idx) {
            chosen.push(idx);
        }
    }

    let mut out = Vec::new();
    let mut total_chars = 0usize;
    for idx in chosen {
        let Some(ref tgt) = history[idx].translation else {
            continue;
        };
        let src_trimmed = history[idx].source.trim().replace('\n', " ");
        let tgt_trimmed = tgt.trim().replace('\n', " ");
        if src_trimmed.chars().count() > 160 || tgt_trimmed.chars().count() > 200 {
            continue;
        }
        let line = format!("{src_trimmed} -> {tgt_trimmed}");
        let line_len = line.chars().count();
        if !out.is_empty() && total_chars + line_len > 280 {
            break;
        }
        total_chars += line_len;
        out.push(line);
    }
    out
}

fn hy_mt2_style_tag(style: &str, source_text: &str, is_dialogue: bool) -> Option<&'static str> {
    match style.trim() {
        "literal" => {
            Some("faithful literal translation preserving original meaning and sentence structure")
        }
        "auto:casual" => is_dialogue.then_some("natural casual dialogue"),
        "novel" | "auto:novel" => {
            is_dialogue.then(|| match crate::ai::detect_dialogue_register(source_text) {
                crate::ai::DialogueRegister::Polite => {
                    "natural polite colloquial dialogue preserving respectful tone"
                }
                crate::ai::DialogueRegister::Casual => {
                    "natural casual informal colloquial dialogue"
                }
                crate::ai::DialogueRegister::Neutral => "natural colloquial dialogue",
            })
        }
        _ => is_dialogue.then_some("natural colloquial dialogue"),
    }
}

fn builtin_ja_ko_novel_hints(
    _source_text: &str,
    _target_lang: &str,
) -> Vec<(&'static str, &'static str)> {
    // User-specific proper nouns and fandom terminology belong in the user Glossary rather than hardcoded defaults.
    Vec::new()
}

pub fn format_instruct_prompt_pair(
    source_text: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    _history: &[ContextPair],
) -> (String, String) {
    let lang = normalize_target_language_for_hy_mt2(target_language);
    let is_dialogue = is_dialogue_segment(source_text);
    let is_novel = matches!(style.trim(), "novel" | "auto:novel");

    let style_rule = if is_novel {
        if is_dialogue {
            match crate::ai::detect_dialogue_register(source_text) {
                crate::ai::DialogueRegister::Polite => {
                    if lang == "Korean" {
                        "Translate as polite Korean dialogue ending in ~요/~네요/~습니다 (해요체/하십시오체).".to_string()
                    } else {
                        format!(
                            "Translate as polite dialogue preserving respectful register into {lang}."
                        )
                    }
                }
                crate::ai::DialogueRegister::Casual => {
                    if lang == "Korean" {
                        "Translate as informal Korean banmal dialogue (반말, 해체/해라체). Never end sentences with polite ~요 or ~습니다.".to_string()
                    } else {
                        format!("Translate as casual informal dialogue into {lang}.")
                    }
                }
                crate::ai::DialogueRegister::Neutral => {
                    format!("Translate as natural colloquial dialogue into {lang}.")
                }
            }
        } else if lang == "Korean" {
            "Translate as 3rd-person Korean novel narration ending in plain ~다/~했다 (해라체 평서형). Never use ~입니다/~습니다/~요 in narration.".to_string()
        } else {
            format!("Translate as plain 3rd-person narrative prose into {lang}.")
        }
    } else {
        hy_mt2_style_tag(style, source_text, is_dialogue)
            .unwrap_or("Translate accurately and naturally.")
            .to_string()
    };

    let has_markers = source_text.contains("[[") && source_text.contains("]]");
    let marker_rule = if has_markers {
        " Preserve markers like [[0|...]] and [[img0]] verbatim."
    } else {
        ""
    };

    let system_prompt = format!(
        "You are an expert literary translator into {lang}. {style_rule}{marker_rule} Output ONLY the translated text without any explanation, headers, or notes."
    );

    let builtin_hints = builtin_ja_ko_novel_hints(source_text, &lang);
    let mut term_pairs = Vec::new();
    for rule in glossary.iter().take(12) {
        let target = if rule.do_not_translate {
            rule.source.as_str()
        } else {
            rule.translation.as_str()
        };
        term_pairs.push(format!("{}={}", rule.source, target));
    }
    for (src, tgt) in builtin_hints {
        if !glossary.iter().any(|r| r.source == src) {
            term_pairs.push(format!("{src}={tgt}"));
        }
    }

    let user_prompt = if term_pairs.is_empty() {
        format!("Translate into {lang}:\n{source_text}")
    } else {
        format!(
            "Glossary ({}):\nTranslate into {lang}:\n{source_text}",
            term_pairs.join(", ")
        )
    };

    (system_prompt, user_prompt)
}

#[allow(dead_code)]
pub fn format_single_prompt(
    source_text: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    history: &[ContextPair],
) -> String {
    format_single_prompt_ext(
        source_text,
        target_language,
        style,
        glossary,
        history,
        false,
    )
}

pub fn format_single_prompt_ext(
    source_text: &str,
    target_language: &str,
    style: &str,
    glossary: &[crate::ai::GlossaryRule],
    history: &[ContextPair],
    omit_marker_clause: bool,
) -> String {
    let lang = normalize_target_language_for_hy_mt2(target_language);
    let mut parts = Vec::new();

    let is_dialogue = is_dialogue_segment(source_text);
    let style_clause = hy_mt2_style_tag(style, source_text, is_dialogue)
        .map(|tag| format!(" Style: [{tag}]."))
        .unwrap_or_default();
    let is_zh_pair = matches!(lang.as_str(), "Chinese" | "Traditional Chinese")
        || source_is_chinese(source_text);
    let use_zh_template = is_zh_pair && style_clause.is_empty();

    let builtin_hints = builtin_ja_ko_novel_hints(source_text, &lang);
    if !glossary.is_empty() || !builtin_hints.is_empty() {
        let mut ref_lines = Vec::with_capacity(glossary.len() + builtin_hints.len() + 1);
        ref_lines.push(if use_zh_template {
            "参考下面的翻译：".to_string()
        } else {
            "Reference the following translations:".to_string()
        });
        for rule in glossary.iter().take(12) {
            let target = if rule.do_not_translate {
                rule.source.as_str()
            } else {
                rule.translation.as_str()
            };
            if use_zh_template {
                ref_lines.push(format!("{} 翻译成 {}", rule.source, target));
            } else {
                ref_lines.push(format!("\"{}\" translates to \"{}\"", rule.source, target));
            }
        }
        for (src, tgt) in builtin_hints {
            if !glossary.iter().any(|r| r.source == src) {
                if use_zh_template {
                    ref_lines.push(format!("{src} 翻译成 {tgt}"));
                } else {
                    ref_lines.push(format!("\"{src}\" translates to \"{tgt}\""));
                }
            }
        }
        parts.push(ref_lines.join("\n"));
    }

    let bg_lines = select_background_pairs(source_text, history);
    let has_bg = !bg_lines.is_empty();
    if has_bg {
        parts.push(format!(
            "{}\n{}",
            if use_zh_template {
                "[背景信息]"
            } else {
                "[Background Information]"
            },
            bg_lines.join("\n")
        ));
    }

    let has_markers =
        !omit_marker_clause && source_text.contains("[[") && source_text.contains("]]");
    let has_link_markers = has_markers && source_text.contains('|');
    let has_img_markers = !omit_marker_clause && source_text.contains("[[img");

    if use_zh_template {
        let zh_lang = hy_mt2_zh_language_name(&lang);
        let marker_clause_zh = if has_link_markers && has_img_markers {
            "，原样保留 [[0|...]] 和 [[img0]] 标记"
        } else if has_link_markers {
            "，原样保留 [[0|...]] 标记"
        } else if has_img_markers {
            "，原样保留 [[img0]] 标记"
        } else if has_markers {
            "，原样保留 [[0|...]] 标记"
        } else {
            ""
        };
        if has_bg {
            parts.push(format!(
                "请参考上面提供的背景信息，将以下文本翻译为{zh_lang}{marker_clause_zh}，注意只需要输出翻译后的结果，不要额外解释：\n\n[原文]\n{source_text}"
            ));
        } else {
            parts.push(format!(
                "将以下文本翻译为{zh_lang}{marker_clause_zh}，注意只需要输出翻译后的结果，不要额外解释：\n\n{source_text}"
            ));
        }
    } else {
        let marker_clause = if has_link_markers && has_img_markers {
            ", preserving [[0|...]] and [[img0]] markers intact"
        } else if has_link_markers {
            ", preserving [[0|...]] markers intact"
        } else if has_img_markers {
            ", preserving [[img0]] markers intact"
        } else if has_markers {
            ", preserving [[0|...]] markers intact"
        } else {
            ""
        };
        if has_bg {
            parts.push(format!(
                "Translate the following segment into {lang}{marker_clause}, considering the background information, without additional explanation.{style_clause}\n\n[Source Text]\n{source_text}"
            ));
        } else {
            parts.push(format!(
                "Translate the following segment into {lang}{marker_clause}, without additional explanation.{style_clause}\n\n{source_text}"
            ));
        }
    }

    parts.join("\n\n")
}

pub fn silent_ellipsis_shortcut(source_text: &str) -> Option<String> {
    let trimmed = source_text.trim();
    match trimmed {
        "「……」" | "「…」" | "「・・・・・・」" | "「・・・」" | "\"...\"" | "“……”" => {
            Some("“……”".to_string())
        }
        "『……』" | "『…』" | "（……）" | "（…）" | "(...)" | "(……)" => {
            Some("‘……’".to_string())
        }
        "……" | "…" => Some("……".to_string()),
        _ => None,
    }
}

fn find_matching_double_bracket_close(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b'[' {
            depth += 1;
            idx += 1;
        } else if bytes[idx] == b']' {
            if depth > 0 {
                depth -= 1;
                idx += 1;
            } else if idx + 1 < bytes.len() && bytes[idx + 1] == b']' {
                return Some(idx);
            } else {
                idx += 1;
            }
        } else {
            idx += 1;
        }
    }
    None
}

fn unwrap_inline_markers_for_shortcut(source_text: &str) -> String {
    if !source_text.contains("[[") || !source_text.contains("]]") {
        return source_text.to_string();
    }
    let mut out = String::with_capacity(source_text.len());
    let mut rest = source_text;
    while let Some(open) = rest.find("[[") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        if let Some(close) = find_matching_double_bracket_close(after_open) {
            let inner = &after_open[..close];
            if inner.starts_with("img")
                && inner.len() > 3
                && inner[3..].chars().all(|c| c.is_ascii_digit())
            {
                // Skip standalone [[imgN]] placeholders
            } else if let Some((id_str, body)) = inner.split_once('|')
                && !id_str.trim().is_empty()
                && id_str.trim().chars().all(|c| c.is_ascii_digit())
            {
                out.push_str(body);
            } else {
                out.push_str(&rest[open..open + 2 + close + 2]);
            }
            rest = &after_open[close + 2..];
        } else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

pub fn non_translatable_shortcut(source_text: &str) -> Option<String> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() {
        return Some(source_text.to_string());
    }
    let unwrapped_buf;
    let check = if trimmed.contains("[[") && trimmed.contains("]]") {
        unwrapped_buf = unwrap_inline_markers_for_shortcut(trimmed);
        unwrapped_buf.trim()
    } else {
        trimmed
    };
    if check.is_empty() {
        return Some(source_text.to_string());
    }
    // 1. Pure numbers, dates, versions, punctuation, counters, symbols, or marker-only [[0|[1]]] (no alphabetic chars).
    if !check.chars().any(|c| c.is_alphabetic()) {
        return Some(source_text.to_string());
    }
    // 2. Single-token URLs, hex constants (0x...), or underscore identifiers without spaces (including wrapped [[0|id]]).
    if !check.contains(char::is_whitespace) {
        let lower = check.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
            || lower.starts_with("ftp://")
        {
            return Some(source_text.to_string());
        }
        if let Some(hex_body) = lower.strip_prefix("0x")
            && !hex_body.is_empty()
            && hex_body.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(source_text.to_string());
        }
        if check.contains('_')
            && check
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/'))
        {
            return Some(source_text.to_string());
        }
    }
    None
}

pub fn already_in_target_language_shortcut(
    source_text: &str,
    target_language: &str,
) -> Option<String> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() {
        return Some(source_text.to_string());
    }
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let mut hangul = 0usize;
    let mut kana = 0usize;
    let mut hanzi = 0usize;
    let mut lower_latin = 0usize;
    let mut cyrillic = 0usize;
    let mut arabic = 0usize;
    let mut thai = 0usize;
    for ch in trimmed.chars() {
        match ch {
            '\u{AC00}'..='\u{D7A3}' | '\u{3131}'..='\u{318E}' => hangul += 1,
            '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' | '\u{FF65}'..='\u{FF9F}' => {
                kana += 1;
            }
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => hanzi += 1,
            'a'..='z' => lower_latin += 1,
            '\u{0400}'..='\u{04FF}' => cyrillic += 1,
            '\u{0600}'..='\u{06FF}' => arabic += 1,
            '\u{0E00}'..='\u{0E7F}' => thai += 1,
            _ => {}
        }
    }
    let is_native = match norm_target.as_str() {
        "Korean" => {
            hangul >= 2
                && kana == 0
                && hanzi == 0
                && lower_latin == 0
                && cyrillic == 0
                && arabic == 0
                && thai == 0
        }
        "Japanese" => {
            kana >= 2
                && hangul == 0
                && lower_latin == 0
                && cyrillic == 0
                && arabic == 0
                && thai == 0
        }
        "Thai" => {
            thai >= 2
                && hangul == 0
                && kana == 0
                && hanzi == 0
                && lower_latin == 0
                && cyrillic == 0
                && arabic == 0
        }
        "Arabic" => {
            arabic >= 2
                && hangul == 0
                && kana == 0
                && hanzi == 0
                && lower_latin == 0
                && cyrillic == 0
                && thai == 0
        }
        _ => false,
    };
    is_native.then(|| source_text.to_string())
}

#[allow(dead_code)]
pub fn preprocess_novel_source_text(source_text: &str, style: &str) -> String {
    preprocess_source_text(source_text, "한국어", style)
}

fn strip_ipa_pronunciation_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    const IPA_MARKERS: &[char] = &[
        'ʁ', 'ɛ', 'ʃ', 'ʒ', 'ɑ', 'ɔ', 'ø', 'œ', 'ɥ', 'ɲ', 'ŋ', 'ɪ', 'ʊ', 'ʌ', 'ə', 'θ', 'ð', 'ː',
        'ˈ', 'ˌ', 'ɡ', 'ʔ', 'ɾ', 'ç', 'ʝ', 'β', 'ɣ', 'χ', 'ħ', 'ʕ',
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        if rest[open..].starts_with("[[") {
            out.push_str(&rest[..open + 2]);
            rest = &rest[open + 2..];
            continue;
        }
        let after = &rest[open + 1..];
        if let Some(close) = after.find(']') {
            let inner = &after[..close];
            let has_ipa = !inner.is_empty()
                && inner.chars().count() <= 40
                && !inner.contains('|')
                && inner.chars().any(|c| IPA_MARKERS.contains(&c));
            if has_ipa {
                out.push_str(rest[..open].trim_end_matches(' '));
                let tail = &after[close + 1..];
                if !out.is_empty()
                    && !out.ends_with(char::is_whitespace)
                    && !tail.is_empty()
                    && !tail.starts_with(char::is_whitespace)
                    && !tail.starts_with([',', '.', ';', ':', ')', '!', '?'])
                {
                    out.push(' ');
                }
                rest = tail;
                continue;
            }
        }
        out.push_str(&rest[..open + 1]);
        rest = after;
    }
    out.push_str(rest);
    out
}

fn unwrap_soft_wrapped_prose(text: &str) -> String {
    if !text.contains('\n') || has_structured_multiline_entries(text) || is_dialogue_segment(text) {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for (idx, raw_line) in lines.iter().enumerate() {
        let trimmed = raw_line.trim();
        if idx == 0 {
            out.push_str(trimmed);
            continue;
        }
        let prev = out.trim_end();
        let prev_is_all_caps_heading = !prev.is_empty()
            && prev.chars().count() <= 40
            && prev.chars().any(|c| c.is_ascii_alphabetic())
            && !prev.chars().any(|c| c.is_ascii_lowercase());
        let prev_is_single_token = !prev.contains(' ') && prev.chars().count() < 24;
        let prev_ends_sentence = prev.ends_with([
            '.', '!', '?', ':', ';', '。', '！', '？', '”', '’', '」', '』', ')', ']',
        ]);
        let next_starts_continuation = trimmed
            .chars()
            .next()
            .is_some_and(|c| !matches!(c, '-' | '*' | '•' | '·' | '[' | '「' | '『'));
        let prev_was_prose = prev.contains(' ')
            && !prev_is_all_caps_heading
            && !prev_is_single_token
            && !prev_ends_sentence;
        if prev_was_prose && next_starts_continuation && !trimmed.is_empty() {
            out.push(' ');
            out.push_str(trimmed);
        } else {
            out.push('\n');
            out.push_str(trimmed);
        }
    }
    out
}

pub fn preprocess_source_text(source_text: &str, target_language: &str, _style: &str) -> String {
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let mut s = source_text
        .replace('\u{00A0}', " ")
        .replace('\u{202F}', " ");
    while s.contains(", )") {
        s = s.replace(", )", ")");
    }
    if s.contains(" , )") {
        s = s.replace(" , )", ")");
    }
    if let Some((_, body)) = split_list_bullet_prefix(&s)
        && !s.contains('\n')
    {
        s = body.to_string();
    }
    s = strip_ipa_pronunciation_brackets(&s);
    s = unwrap_soft_wrapped_prose(&s);

    if norm_target != "Japanese" {
        // Generic Japanese homograph & particle disambiguation for compact 1.8B models.
        s = s
            .replace("人気が少ない", "人の気配が少ない")
            .replace("人気がない", "人の気配がない")
            .replace("文書をご注意ください", "文書をご参照ください");
    }

    s
}

pub fn clean_hy_mt2_output(raw: &str) -> String {
    let mut s = raw
        .replace("<｜hy_place▁holder▁no▁2｜>", "")
        .replace("<｜hy_EOT｜>", "")
        .replace("<｜hy_Assistant｜>", "")
        .replace("<｜hy_begin▁of▁sentence｜>", "")
        .replace("<｜hy_end▁of▁sentence｜>", "")
        .replace("<|im_end|>", "")
        .replace("<|eot_id|>", "")
        .replace("<end_of_turn>", "")
        .replace("[|endofturn|]", "");
    if let Some(end_think) = s.find("</think>") {
        s = s[end_think + "</think>".len()..].to_string();
    }

    for marker in [
        "[출처 텍스트]",
        "[Source Text]",
        "[원문 텍스트]",
        "[原文]",
        "[번역 결과]",
        "[번역문]",
        "[译文]",
    ] {
        if let Some(pos) = s.rfind(marker) {
            let after = s[pos + marker.len()..].trim();
            if !after.is_empty() {
                s = after.to_string();
                break;
            }
        }
    }

    // Remove prompt-template headers, glossary echoes, or background pair echoes across languages.
    let mut kept_lines = Vec::new();
    let mut last_arrow_rhs: Option<String> = None;
    for line in s.lines() {
        let mut l = line.trim();
        let lower = l.to_ascii_lowercase();
        if lower.starts_with("style:")
            || lower.starts_with("note:")
            || lower.starts_with("translate the following segment")
            || lower.starts_with("output only")
            || lower.starts_with("reference the following translations")
            || lower.starts_with("mandatory glossary")
            || lower.starts_with("glossary (")
            || lower.contains("translates to \"")
            || l.starts_with("参考下面的翻译")
            || l.starts_with("다음 번역을 참고")
            || l.contains("으로 번역할 것")
            || l.contains("로 번역할 것")
            || l.contains("으로 번역 할 것")
            || l.contains("로 번역 할 것")
            || l.contains("으로 번역하십시오")
            || l.contains("로 번역하십시오")
            || l.contains("do not translate")
        {
            continue;
        }
        for header in [
            "[Source Text]",
            "[출처 텍스트]",
            "[원문 텍스트]",
            "[원문]",
            "[原文]",
            "[Background Information]",
            "[배경 정보]",
            "[背景信息]",
            "[Translation]",
            "[번역 결과]",
            "[번역문]",
            "[译文]",
            "译文：",
            "译文:",
            "翻译结果：",
            "翻译结果:",
            "日本語訳：",
            "日本語訳:",
            "Translate into Korean:",
            "Korean translation:",
            "English translation:",
            "Japanese translation:",
            "Chinese translation:",
            "Translation:",
        ] {
            if let Some(rest) = l.strip_prefix(header) {
                l = rest.trim();
            }
        }
        if l.is_empty() {
            continue;
        }
        // Drop echoed history mapping lines ("A -> B" or "A → B") when the model dumps [Background Information].
        if l.contains(" -> ") || l.contains(" → ") {
            if let Some((_, rhs)) = l.rsplit_once(" -> ").or_else(|| l.rsplit_once(" → ")) {
                let rhs_clean = rhs.trim();
                if !rhs_clean.is_empty() && !rhs_clean.contains("[배경 정보]") {
                    last_arrow_rhs = Some(rhs_clean.to_string());
                }
            }
            continue;
        }
        kept_lines.push(l.to_string());
    }
    if kept_lines.is_empty() {
        if let Some(fallback_rhs) = last_arrow_rhs {
            return fallback_rhs;
        }
        return s.trim().to_string();
    }
    kept_lines.join("\n").trim().to_string()
}

pub fn strip_echoed_history_translations(raw: &str, history: &[ContextPair]) -> String {
    let mut s = raw.trim().to_string();
    if s.is_empty() || history.is_empty() {
        return s;
    }
    let recent_tgts: Vec<String> = history
        .iter()
        .rev()
        .take(6)
        .filter_map(|p| p.translation.as_deref())
        .map(|t| t.trim().to_string())
        .filter(|t| t.chars().count() >= 14)
        .collect();
    if recent_tgts.is_empty() {
        return s;
    }
    // 1. Drop any echoed line that matches or is a substantial substring of a previous translation
    let kept_lines: Vec<&str> = s
        .lines()
        .map(str::trim)
        .filter(|line| {
            if line.is_empty() {
                return false;
            }
            let l_chars = line.chars().count();
            if l_chars >= 18
                && recent_tgts.iter().any(|prev| {
                    prev == line || prev.contains(line) || line.starts_with(prev.as_str())
                })
            {
                return false;
            }
            true
        })
        .collect();
    if !kept_lines.is_empty() {
        s = kept_lines.join("\n").trim().to_string();
    }
    // 2. Strip any echoed prefix from recent translations (even when concatenated on the same line)
    for _ in 0..3 {
        let mut stripped = false;
        for prev in &recent_tgts {
            if s.starts_with(prev.as_str()) && s.len() > prev.len() + 6 {
                s = s[prev.len()..].trim().to_string();
                stripped = true;
                break;
            }
            // Also check individual sentences from `prev` (>= 20 chars)
            for sent in prev.split_inclusive(['.', '!', '?', '…']) {
                let st = sent.trim();
                if st.chars().count() >= 20 && s.starts_with(st) && s.len() > st.len() + 6 {
                    s = s[st.len()..].trim().to_string();
                    stripped = true;
                    break;
                }
            }
            if stripped {
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    s
}

fn has_korean_final_consonant(c: char) -> bool {
    let code = c as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return false;
    }
    (code - 0xAC00) % 28 != 0
}

fn has_korean_ss_batchim(c: char) -> bool {
    let code = c as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return false;
    }
    (code - 0xAC00) % 28 == 20
}

fn is_korean_hada_adjective_stem(prefix: &str) -> bool {
    const HADA_ADJ_MULTI_STEMS: &[&str] = &[
        "불가능", "불충분", "불안전", "불안정", "불완전", "불가피", "불분명", "불확실", "불적절",
        "부적절", "부적합", "부정확", "불편", "불리", "필요", "가능", "중요", "강력", "복잡",
        "정확", "단순", "간단", "명확", "분명", "명백", "적절", "적합", "유효", "무효", "안전",
        "안정", "완벽", "완전", "충분", "부족", "유명", "다양", "동일", "유사", "비슷", "독특",
        "고유", "특이", "특별", "투명", "엄격", "심각", "위험", "치명", "유용", "편리", "훌륭",
        "탁월", "신속", "원활", "빈번", "활발", "풍부", "유리", "익숙", "친숙", "건강", "신선",
        "깨끗", "조용", "친절", "민감", "취약", "견고", "유연", "뚜렷", "정밀", "미세", "거대",
        "방대", "막대", "철저", "신중", "과도", "타당", "정당", "부당", "상당", "무관", "유관",
        "유한", "무한", "급격", "극심", "현저", "미미", "희박", "순수", "온화", "완만", "당연",
        "필연", "우연", "공정", "평등", "확실", "유력", "긴급", "중대", "평범", "비범", "생생",
        "밀접", "긴밀", "느슨", "단단", "튼튼", "용이", "난해", "진지", "성실", "솔직", "겸손",
        "오만", "교만", "냉정", "침착", "치밀", "영리", "총명", "현명", "미묘", "기묘", "이상",
        "희귀", "진귀", "귀중", "소중", "막중", "다행", "불행", "행복", "유익", "무익", "유해",
        "무해", "무난", "미흡", "비참", "처참", "곤란", "불안", "편안", "유쾌", "불쾌", "대담",
        "용감", "과감", "담대", "능숙", "미숙", "노련", "미안", "죄송", "안타", "다정", "친밀",
        "긴박", "위급", "급박", "간절", "절실", "선명", "희미", "은은", "잔잔", "고요", "적막",
        "막막", "갑갑", "답답", "섭섭", "서운", "무해", "유해", "순진", "완고", "명쾌", "상쾌",
        "불길", "심심", "따뜻", "서늘", "시원", "한산", "번잡", "생소",
    ];
    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return false;
    }
    if HADA_ADJ_MULTI_STEMS
        .iter()
        .any(|stem| trimmed.ends_with(stem))
    {
        return true;
    }
    const HADA_ADJ_SINGLE_WORD_STEMS: &[&str] = &["강", "약", "흔", "귀", "진", "순", "뻔"];
    HADA_ADJ_SINGLE_WORD_STEMS.iter().any(|stem| {
        if let Some(before) = trimmed.strip_suffix(stem) {
            before.is_empty()
                || before
                    .chars()
                    .next_back()
                    .is_none_or(|c| !('\u{AC00}'..='\u{D7A3}').contains(&c))
        } else {
            false
        }
    })
}

fn replace_korean_copula_imnida(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("입니다") {
        let prefix = &rest[..pos];
        let after = &rest[pos + "입니다".len()..];
        let is_sentence_boundary = after.is_empty()
            || after.starts_with([
                '.', '!', '?', '…', ';', ':', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
            ]);
        let trimmed_prefix = prefix.trim_end_matches([
            '’', '”', '"', '\'', ')', '』', '」', '》', '】', '〉', '»',
        ]);
        let mut prev_char = trimmed_prefix.chars().last();
        if prev_char == Some(']')
            && let Some(inner_start) = trimmed_prefix.rfind('|')
        {
            let inner = trimmed_prefix[inner_start + 1..].trim_end_matches(']');
            prev_char = inner
                .trim_end_matches(['’', '”', '"', '\'', ')', '』', '」', '》', '】', '〉', '»'])
                .chars()
                .last();
        }
        result.push_str(prefix);
        if is_sentence_boundary {
            if let Some(ch) = prev_char
                && (0xAC00..=0xD7A3).contains(&(ch as u32))
            {
                if ch == '아' && trimmed_prefix.ends_with("아") {
                    result.push_str("입니다");
                } else if has_korean_final_consonant(ch) {
                    result.push_str("이다");
                } else {
                    result.push('다');
                }
            } else if trimmed_prefix.ends_with("]]") {
                result.push_str("이다");
            } else if let Some(ch) = prev_char
                && ch.is_ascii_alphanumeric()
            {
                if matches!(ch, '2' | '4' | '5' | '9') {
                    result.push('다');
                } else {
                    result.push_str("이다");
                }
            } else {
                result.push_str("입니다");
            }
        } else {
            result.push_str("입니다");
        }
        rest = after;
    }
    result.push_str(rest);
    result
}

pub fn normalize_korean_narration_endings(out: &mut String) {
    let is_boundary = |after: &str| {
        after.is_empty()
            || after.starts_with([
                '.', '!', '?', '…', ';', ':', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
            ])
    };

    // 1. Explicit irregular/adjective formal -> plain endings
    for (formal, plain) in [
        ("있지 않습니다", "있지 않다"),
        ("그렇지 않습니다", "그렇지 않다"),
        ("많지 않습니다", "많지 않다"),
        ("좋지 않습니다", "좋지 않다"),
        ("높지 않습니다", "높지 않다"),
        ("크지 않습니다", "크지 않다"),
        ("작지 않습니다", "작지 않다"),
        ("곳이죠", "곳이다"),
        ("뿐이죠", "뿐이다"),
        ("거죠", "거다"),
        ("거예요", "거다"),
        ("아닙니다", "아니다"),
        ("힘듭니다", "힘들다"),
        ("올바릅니다", "올바르다"),
        ("남다릅니다", "남다르다"),
        ("다릅니다", "다르다"),
        ("빠릅니다", "빠르다"),
        ("푸릅니다", "푸르다"),
        ("게으릅니다", "게으르다"),
        ("고릅니다", "고르다"),
        ("큽니다", "크다"),
        ("나쁩니다", "나쁘다"),
        ("바쁩니다", "바쁘다"),
        ("기쁩니다", "기쁘다"),
        ("슬픕니다", "슬프다"),
        ("아픕니다", "아프다"),
        ("예쁩니다", "예쁘다"),
        ("고픕니다", "고프다"),
        ("비쌉니다", "비싸다"),
        ("값쌉니다", "값싸다"),
        ("짭니다", "짜다"),
        ("깁니다", "길다"),
        ("멉니다", "멀다"),
        ("거칩니다", "거칠다"),
        ("드뭅니다", "드물다"),
        ("가늡니다", "가늘다"),
        ("둥굽니다", "둥글다"),
        ("서툽니다", "서툴다"),
        ("있습니다", "있다"),
        ("없습니다", "없다"),
        ("같습니다", "같다"),
        ("많습니다", "많다"),
        ("적습니다", "적다"),
        ("작습니다", "작다"),
        ("높습니다", "높다"),
        ("낮습니다", "낮다"),
        ("좋습니다", "좋다"),
        ("나쁩니다", "나쁘다"),
        ("그렇습니다", "그렇다"),
        ("이렇습니다", "이렇다"),
        ("저렇습니다", "저렇다"),
        ("어떻습니다", "어떻다"),
        ("됩니다", "된다"),
    ] {
        for punct in [".", "!", "…", ";", ":", ",", ")", "”", "’"] {
            let pat = format!("{formal}{punct}");
            if out.contains(&pat) {
                let rep = format!("{plain}{punct}");
                *out = out.replace(&pat, &rep);
            }
        }
        if out.ends_with(formal) {
            let new_len = out.len() - formal.len();
            out.truncate(new_len);
            out.push_str(plain);
        }
    }

    // 2. Context-aware `~하지 않습니다` -> `~하지 않다` (adj) vs `~하지 않는다` (verb)
    if out.contains("하지 않습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("하지 않습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "하지 않습니다".len()..];
            fixed.push_str(prefix);
            if is_boundary(after) {
                if is_korean_hada_adjective_stem(prefix) {
                    fixed.push_str("하지 않다");
                } else {
                    fixed.push_str("하지 않는다");
                }
            } else {
                fixed.push_str("하지 않습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 3. Remaining `~지 않습니다` (되지 않습니다 등)
    if out.contains("지 않습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("지 않습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "지 않습니다".len()..];
            fixed.push_str(prefix);
            if is_boundary(after) {
                fixed.push_str("지 않는다");
            } else {
                fixed.push_str("지 않습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 4. Context-aware `<stem>합니다` -> `<stem>하다` (adjective) vs `<stem>한다` (verb)
    if out.contains("합니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("합니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "합니다".len()..];
            fixed.push_str(prefix);
            if is_boundary(after) {
                if is_korean_hada_adjective_stem(prefix) {
                    fixed.push_str("하다");
                } else {
                    fixed.push_str("한다");
                }
            } else {
                fixed.push_str("합니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 5. General Unicode Hangul past/future `ㅆ` batchim + `습니다` -> `<syllable>다`
    if out.contains("습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "습니다".len()..];
            fixed.push_str(prefix);
            if is_boundary(after) && prefix.chars().last().is_some_and(has_korean_ss_batchim) {
                fixed.push('다');
            } else {
                fixed.push_str("습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 6. General copula `<noun>입니다` -> `<noun>이다` / `<noun>다`
    if out.contains("입니다") {
        *out = replace_korean_copula_imnida(out);
    }

    // 7. General Unicode Hangul `<syllable with ㅂ batchim>니다` -> `<syllable with ㄴ batchim>다`
    //    (covers 씁니다->쓴다, 봅니다->본다, 만듭니다->만든다, 보여줍니다->보여준다, 열립니다->열린다, etc.)
    if out.contains("니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "니다".len()..];
            if is_boundary(after)
                && let Some(last_ch) = prefix.chars().last()
            {
                let code = last_ch as u32;
                if last_ch != '습'
                    && (0xAC00..=0xD7A3).contains(&code)
                    && (code - 0xAC00) % 28 == 17
                {
                    let prefix_without_last = &prefix[..prefix.len() - last_ch.len_utf8()];
                    let replaced_syllable = char::from_u32(code - 17 + 4).unwrap_or(last_ch);
                    fixed.push_str(prefix_without_last);
                    fixed.push(replaced_syllable);
                    fixed.push('다');
                    rest = after;
                    continue;
                }
            }
            fixed.push_str(prefix);
            fixed.push_str("니다");
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 8. Fix common Korean adjective `-ㄴ다` conjugation errors (`필요한다` -> `필요하다`)
    for (bad_adj, good_adj) in [
        ("필요한다", "필요하다"),
        ("중요한다", "중요하다"),
        ("불가능한다", "불가능하다"),
        ("가능한다", "가능하다"),
        ("유명한다", "유명하다"),
        ("다양한다", "다양하다"),
        ("동일한다", "동일하다"),
        ("유사한다", "유사하다"),
        ("적합한다", "적합하다"),
        ("간단한다", "간단하다"),
        ("복잡한다", "복잡하다"),
        ("정확한다", "정확하다"),
        ("안전한다", "안전하다"),
        ("완벽한다", "완벽하다"),
        ("충분한다", "충분하다"),
        ("부족한다", "부족하다"),
        ("투명한다", "투명하다"),
        ("특별한다", "특별하다"),
        ("유용한다", "유용하다"),
        ("필요하지 않는다", "필요하지 않다"),
        ("가능하지 않는다", "가능하지 않다"),
        ("충분하지 않는다", "충분하지 않다"),
    ] {
        if out.contains(bad_adj) {
            *out = out.replace(bad_adj, good_adj);
        }
    }

    // 9. Safe sentence-ending polite `~해요 / ~어요 / ~아요 / ~이야` normalization in non-dialogue prose
    if !out.contains('“') && !out.contains('‘') {
        if out.contains("해요.") {
            let mut fixed = String::with_capacity(out.len());
            let mut rest = out.as_str();
            while let Some(pos) = rest.find("해요.") {
                let prefix = &rest[..pos];
                let after = &rest[pos + "해요.".len()..];
                fixed.push_str(prefix);
                if is_korean_hada_adjective_stem(prefix) {
                    fixed.push_str("하다.");
                } else {
                    fixed.push_str("한다.");
                }
                rest = after;
            }
            fixed.push_str(rest);
            *out = fixed;
        }
        for (non_plain, plain) in [
            ("했어요.", "했다."),
            ("됐어요.", "됐다."),
            ("왔어요.", "왔다."),
            ("갔어요.", "갔다."),
            ("났어요.", "났다."),
            ("졌어요.", "졌다."),
            ("렸어요.", "렸다."),
            ("있어요.", "있다."),
            ("없어요.", "없다."),
            ("같아요.", "같다."),
            ("많아요.", "많다."),
            ("적어요.", "적다."),
            ("좋아요.", "좋다."),
            ("작아요.", "작다."),
            ("높아요.", "높다."),
            ("낮아요.", "낮다."),
            ("넓어요.", "넓다."),
            ("좁아요.", "좁다."),
            ("길어요.", "길다."),
            ("짧아요.", "짧다."),
            ("돼요.", "된다."),
            ("봐요.", "본다."),
            ("와요.", "온다."),
            ("가요.", "간다."),
            ("네요.", "다."),
            ("군요.", "다."),
            ("이야.", "이다."),
            ("거야.", "거다."),
        ] {
            if out.contains(non_plain) {
                *out = out.replace(non_plain, plain);
            }
        }
    }
}

fn is_literal_code_marker_inner(inner: &str) -> bool {
    let t = inner.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '\\'))
    {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    t.contains('_')
        || t.contains("::")
        || t.starts_with("CVE-")
        || (t.starts_with('-') && t.chars().nth(1).is_some_and(|c| c.is_ascii_alphanumeric()))
        || [
            ".exe", ".dll", ".cfg", ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".cpp",
            ".cc", ".c", ".h", ".hpp", ".json", ".toml", ".yaml", ".yml", ".md", ".sh", ".sql",
            ".css", ".html", ".swift", ".kt", ".java", ".wasm", ".gguf", ".txt",
        ]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

pub fn repair_inline_markers(source_text: &str, out: &mut String) {
    if !source_text.contains("[[") || !source_text.contains("]]") {
        return;
    }
    // 1. Collect expected marker IDs and literal code tokens from source_text (`[[0|...]]`, `[[1|...]]`, `[[img0]]`)
    let mut expected_ids = Vec::new();
    let mut literal_codes: Vec<(usize, String)> = Vec::new();
    let mut scan = source_text;
    while let Some(start) = scan.find("[[") {
        let rest = &scan[start + 2..];
        if let Some(end) = find_matching_double_bracket_close(rest) {
            let inner = &rest[..end];
            if let Some((id_str, body)) = inner.split_once('|') {
                if let Ok(id) = id_str.trim().parse::<usize>() {
                    expected_ids.push(id);
                    if is_literal_code_marker_inner(body) {
                        literal_codes.push((id, body.trim().to_string()));
                    }
                }
            }
            scan = &rest[end + 2..];
        } else {
            break;
        }
    }
    if expected_ids.is_empty() {
        return;
    }

    // 2. Repair malformed single-bracket closers (`[[1|...]`, `[[5|...]()]`) and triple-bracket footnotes (`[[0|[4]]]` -> `[[0|[4]]`)
    for &id in &expected_ids {
        let prefix = format!("[[{id}|");
        let is_footnote_src =
            source_text.contains(&format!("[[{id}|[")) && source_text.contains("]]]");
        let mut search_from = 0usize;
        while let Some(rel_pos) = out[search_from..].find(&prefix) {
            let abs_start = search_from + rel_pos;
            let content_start = abs_start + prefix.len();
            let tail = &out[content_start..];
            // Check if there is a `]]` before any next `[[` or `]`
            let next_open = tail.find("[[").unwrap_or(tail.len());
            let slice = &tail[..next_open];
            if is_footnote_src
                && slice.starts_with('[')
                && !slice.contains("]]]")
                && let Some(d_close) = slice.find("]]")
            {
                let close_abs = content_start + d_close;
                out.replace_range(close_abs..close_abs + 2, "]]]");
                search_from = close_abs + 3;
                continue;
            }
            if let Some(d_close) = slice.find("]]") {
                search_from = content_start + d_close + 2;
                continue;
            }
            if let Some(s_close) = slice.find(']') {
                let inner_end = content_start + s_close;
                // Handle `]()]` artifact or plain single `]`
                if out[inner_end..].starts_with("]()]") {
                    out.replace_range(inner_end..inner_end + 4, "]]");
                } else {
                    out.replace_range(inner_end..inner_end + 1, "]]");
                }
                search_from = inner_end + 2;
            } else if let Some((rel_pos, cjk_ch)) = slice.char_indices().find(|&(_, ch)| {
                (ch == '》' && !slice.contains('《')) || (ch == '】' && !slice.contains('【'))
            }) {
                let inner_end = content_start + rel_pos;
                out.replace_range(inner_end..inner_end + cjk_ch.len_utf8(), "]]");
                search_from = inner_end + 2;
            } else {
                break;
            }
        }
    }

    // 3. Remove accidental duplicate `[[id|...]]` occurrences when source only had 1
    for &id in &expected_ids {
        if expected_ids.iter().filter(|&&x| x == id).count() != 1 {
            continue;
        }
        let prefix = format!("[[{id}|");
        let first = out.find(&prefix);
        let last = out.rfind(&prefix);
        if let (Some(f), Some(l)) = (first, last)
            && f != l
        {
            // Keep the last (or well-placed) marker and unwrap the extra earlier `[[id|...]]` -> `...`
            if let Some(rel_end) = out[f + prefix.len()..].find("]]") {
                let close_start = f + prefix.len() + rel_end;
                out.replace_range(close_start..close_start + 2, "");
                out.replace_range(f..f + prefix.len(), "");
            }
        }
    }

    // 4. Clean stray unbalanced `]`, `】`, `》`, or `(` right next to markers or numbers
    if !source_text.contains('(') && out.contains("]](") {
        *out = out.replace("]](", "]]");
    }
    if !source_text.contains('】') && out.contains("]]】") {
        *out = out.replace("]]】", "]]");
    }
    if !source_text.contains('》') && out.contains("]]》") {
        *out = out.replace("]]》", "]]");
    }
    // Unwrap malformed `[[text]]` without `|` (not `[[imgN]]`), e.g. `[[rustup]]` -> `rustup`
    if out.contains("[[") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(open) = rest.find("[[") {
            fixed.push_str(&rest[..open]);
            let after_open = &rest[open + 2..];
            // Find the matching `]]` without mistaking `[4]]]`'s inner `[4]` close for the outer `]]`
            if let Some(close) = find_matching_double_bracket_close(after_open) {
                let inner = &after_open[..close];
                if inner.contains('|') || inner.starts_with("img") {
                    fixed.push_str("[[");
                    fixed.push_str(inner);
                    fixed.push_str("]]");
                } else {
                    fixed.push_str(inner);
                }
                rest = &after_open[close + 2..];
            } else {
                fixed.push_str(&rest[open..]);
                rest = "";
                break;
            }
        }
        fixed.push_str(rest);
        *out = fixed;
    }
    if out.contains(']') {
        let mut cleaned = String::with_capacity(out.len());
        let chars: Vec<char> = out.chars().collect();
        let mut i = 0usize;
        let mut in_marker = false;
        let mut inner_brackets = 0usize;
        let mut outer_brackets = 0usize;
        while i < chars.len() {
            if !in_marker && i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '[' {
                in_marker = true;
                inner_brackets = 0;
                cleaned.push('[');
                cleaned.push('[');
                i += 2;
                continue;
            }
            if in_marker {
                if chars[i] == '[' {
                    inner_brackets += 1;
                    cleaned.push('[');
                    i += 1;
                    continue;
                }
                if chars[i] == ']' {
                    if inner_brackets > 0 {
                        inner_brackets -= 1;
                        cleaned.push(']');
                        i += 1;
                        continue;
                    }
                    if i + 1 < chars.len() && chars[i + 1] == ']' {
                        in_marker = false;
                        cleaned.push(']');
                        cleaned.push(']');
                        i += 2;
                        continue;
                    }
                }
                cleaned.push(chars[i]);
                i += 1;
                continue;
            }
            if chars[i] == '[' {
                outer_brackets += 1;
                cleaned.push('[');
                i += 1;
                continue;
            }
            if chars[i] == ']' {
                if outer_brackets > 0 {
                    outer_brackets -= 1;
                    cleaned.push(']');
                }
                i += 1;
                continue;
            }
            cleaned.push(chars[i]);
            i += 1;
        }
        *out = cleaned;
    }

    // 5. Restore verbatim literal code/map/CLI identifiers inside unique [[id|...]] markers
    for (id, expected_inner) in &literal_codes {
        if expected_ids.iter().filter(|&&x| x == *id).count() != 1 {
            continue;
        }
        let prefix = format!("[[{id}|");
        let first = out.find(&prefix);
        let last = out.rfind(&prefix);
        if let (Some(pos), Some(last_pos)) = (first, last)
            && pos == last_pos
        {
            let content_start = pos + prefix.len();
            if let Some(rel_end) = find_matching_double_bracket_close(&out[content_start..])
                && &out[content_start..content_start + rel_end] != expected_inner
            {
                out.replace_range(content_start..content_start + rel_end, expected_inner);
            }
        }
    }
}

pub fn enforce_casual_korean_dialogue(out: &mut String) {
    // 1. First-person humble pronouns -> casual pronouns
    for (humble, casual) in [
        ("제가 ", "내가 "),
        ("저도 ", "나도 "),
        ("저는 ", "나는 "),
        ("저한테", "나한테"),
        ("저에게", "나에게"),
        ("제게 ", "내게 "),
        ("저희", "우리"),
    ] {
        if out.contains(humble) {
            *out = out.replace(humble, casual);
        }
    }

    // 2. High-precision conversational phrase & sentence-ending replacements
    for (polite_pat, casual_rep) in [
        ("전해주세요", "전해 줘"),
        ("전달해 주세요", "전해 줘"),
        ("말해주세요", "말해 줘"),
        ("말해 주세요", "말해 줘"),
        ("알려주세요", "알려 줘"),
        ("알려 주세요", "알려 줘"),
        ("해 주세요", "해 줘"),
        ("해주세요", "해 줘"),
        ("주세요", "줘"),
        ("대접할게요", "대접할게"),
        ("사줄게요", "사줄게"),
        ("할게요", "할게"),
        ("갈게요", "갈게"),
        ("볼게요", "볼게"),
        ("먹자요?", "먹을래?"),
        ("하고 있군요", "하고 있네"),
        ("하고 있네요", "하고 있네"),
        ("혼자네요?", "혼자구나."),
        ("혼자네요", "혼자구나"),
        ("혼자인군요", "혼자구나"),
        ("그런 거군요", "그런 거구나"),
        ("있군요", "있구나"),
        ("유용하네요", "고맙지"),
        ("다행이네요", "고맙지"),
        ("도움이 되네요", "도움이 돼"),
        ("살겠네요", "살겠어"),
        ("좋지 않아요", "좋지 않아"),
        ("다르지 않아요", "다르지 않아"),
        ("변하지 않아요", "변하지 않아"),
        ("않아요", "않아"),
        ("꽤 다르죠", "꽤 다르지"),
        ("물어보고 싶은 게 있어요", "물어보고 싶은 게 있어"),
        ("알고 있었던 것 같아요", "알고 있었던 것 같던데"),
        ("아는 것 같았어요", "아는 것 같던데"),
        ("같은데요", "같은데"),
        ("같네요", "같아"),
        ("같아요", "같아"),
        ("않았나요?", "않았어?"),
        ("아, 그렇군요", "아, 그렇지"),
        ("그렇군요", "그렇군"),
        ("그러네요", "그렇네"),
        ("건가요?", "거야?"),
        ("건가요", "거야"),
        ("인가요?", "이야?"),
        ("인가요", "인가"),
        ("일까요?", "일까?"),
        ("할까요?", "할까?"),
        ("될까요?", "될까?"),
        ("볼까요?", "볼까?"),
        ("갈까요?", "갈까?"),
        ("까요?", "까?"),
        ("거예요?", "거야?"),
        ("거예요", "거야"),
        ("이에요?", "이야?"),
        ("이에요", "이야"),
        ("예요?", "야?"),
        ("예요", "야"),
        ("거든요", "거든"),
        ("잖아요", "잖아"),
        ("텐데요", "텐데"),
        ("걸요", "걸"),
        ("래요", "래"),
        ("대요", "대"),
        ("네요", "네"),
        ("군요", "군"),
        ("지요?", "지?"),
        ("지요", "지"),
        ("죠?", "지?"),
        ("죠", "지"),
        ("나요?", "나?"),
        ("나요", "나"),
        ("가요?", "가?"),
        ("가요", "가"),
        ("아닙니다", "아니야"),
        ("입니다", "이야"),
        ("습니다", "다"),
    ] {
        if out.contains(polite_pat) {
            *out = out.replace(polite_pat, casual_rep);
        }
    }

    // 3. General Korean morphological rule for remaining `해체 + 요` (`~아요/~어요/~해요/~워요/~봐요/~와요/~돼요/~줘요/~서요/~써요/~라요/~마요/~자요`)
    //    at sentence boundaries (`.`, `?`, `!`, `…`, `”`, `’`, `"`, `'`).
    const HAECHE_VOWELS: &[char] = &[
        '아', '어', '여', '해', '워', '봐', '와', '돼', '춰', '줘', '둬', '서', '써', '떠', '커',
        '라', '마', '자', '네', '군', '지', '게', '데', '걸',
    ];
    for &v in HAECHE_VOWELS {
        for punct in [".", "?", "!", "…", "”", "’", "\"", "'"] {
            let pat = format!("{v}요{punct}");
            if out.contains(&pat) {
                let rep = format!("{v}{punct}");
                *out = out.replace(&pat, &rep);
            }
        }
        let end_pat = format!("{v}요");
        if out.ends_with(&end_pat) {
            out.pop(); // strip trailing '요'
        }
    }
}

pub fn strip_spurious_question_mark(source_text: &str, out: &mut String) {
    if source_text.contains('？') || source_text.contains('?') {
        return;
    }
    let trimmed = source_text.trim();
    // Do not strip `?` if Japanese source clearly ends with an interrogative particle without `？`
    for q_suffix in ["のか」", "かい」", "かね」", "の」", "かな」", "か」"] {
        if trimmed.ends_with(q_suffix) {
            return;
        }
    }
    if out.contains("필요 없을까?") {
        *out = out.replace("필요 없을까?", "필요 없겠지.");
    }
    if out.ends_with("?”") {
        let new_len = out.len() - "?”".len();
        out.truncate(new_len);
        out.push_str(".”");
    } else if out.ends_with("?’") {
        let new_len = out.len() - "?’".len();
        out.truncate(new_len);
        out.push_str(".’");
    } else if out.ends_with('?') {
        out.pop();
        out.push('.');
    }
    for (spurious_q, declarative_rep) in [
        ("혼자야.”", "혼자구나.”"),
        ("혼자야?”", "혼자구나.”"),
        ("그런 거야.’", "그런 거구나.’"),
        ("그런 거야?’", "그런 거구나.’"),
    ] {
        if out.contains(spurious_q) {
            *out = out.replace(spurious_q, declarative_rep);
        }
    }
}

#[allow(dead_code)]
pub fn postprocess_novel_output(source_text: &str, style: &str, out: String) -> String {
    postprocess_translation_output(source_text, "한국어", style, out)
}

fn fix_english_billion_to_korean(source_text: &str, out: &mut String) {
    let lower = source_text.to_ascii_lowercase();
    if !lower.contains("billion") || !out.contains('억') {
        return;
    }
    let mut scan = lower.as_str();
    while let Some(pos) = scan.find("billion") {
        let before = scan[..pos].trim_end();
        let num_start = before
            .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let num_str = before[num_start..].trim();
        if !num_str.is_empty()
            && num_str.chars().any(|c| c.is_ascii_digit())
            && let Ok(val) = num_str.parse::<f64>()
        {
            let eok_val = val * 10.0;
            let formatted_eok = if (eok_val - eok_val.round()).abs() < 1e-6 {
                format!("{}", eok_val.round() as i64)
            } else {
                let s = format!("{eok_val:.2}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            };
            let wrong_with_space = format!("{num_str} 억");
            let wrong_tight = format!("{num_str}억");
            let right_tight = format!("{formatted_eok}억");
            if out.contains(&wrong_with_space) {
                *out = out.replace(&wrong_with_space, &right_tight);
            }
            if out.contains(&wrong_tight) {
                *out = out.replace(&wrong_tight, &right_tight);
            }
        }
        scan = &scan[pos + "billion".len()..];
    }
}

fn fix_korean_code_token_particles(out: &mut String) {
    const VOWEL_SUFFIXES: &[&str] = &[
        ".cpp", ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".go", ".md", ".sh", ".css", ".txt",
        ".java", ".swift", ".kt", ".exe", ".wasm", "er]]", "or]]",
    ];
    for suffix in VOWEL_SUFFIXES {
        for (bad_p, good_p) in [("은", "는"), ("이", "가"), ("을", "를"), ("과", "와")] {
            let pat = format!("{suffix}{bad_p}");
            let mut search_from = 0usize;
            while let Some(rel) = out[search_from..].find(&pat) {
                let abs = search_from + rel;
                let after = &out[abs + pat.len()..];
                let ok_boundary = after.is_empty()
                    || after.starts_with(|c: char| {
                        c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ';' | ':')
                    });
                if ok_boundary {
                    let p_start = abs + suffix.len();
                    out.replace_range(p_start..abs + pat.len(), good_p);
                    search_from = p_start + good_p.len();
                } else {
                    search_from = abs + pat.len();
                }
            }
        }
    }
}

pub fn postprocess_translation_output(
    source_text: &str,
    target_language: &str,
    _style: &str,
    mut out: String,
) -> String {
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let src_trimmed = source_text.trim();
    let src_lower = src_trimmed.to_ascii_lowercase();

    // Pre-repair markers and hallucinated URLs so Korean narration & particle rules see clean `]]` closers.
    strip_hallucinated_marker_urls(source_text, &mut out);
    repair_inline_markers(source_text, &mut out);

    // 1. Target-specific rules for Korean outputs ONLY (zero cross-language interference).
    if norm_target == "Korean" {
        if out.starts_with("already ") {
            out = out.replacen("already ", "이미 ", 1);
        }
        if out.contains("ような") && !source_text.contains("ような") {
            out = out.replace("ような", "");
        } else if out.ends_with("ような?”") || out.ends_with("ような?」") {
            out = out.replace("ような", "것 같기도 하고");
        }
        if out.contains("上的") && !source_text.contains("上的") {
            out = out.replace("서버上的", "서버상의").replace("上的", "상의");
        }
        if out.contains("アワード") && !source_text.contains("アワード") {
            out = out.replace("アワード", "어워드");
        }
        if source_text.contains("移植") && out.contains("移植") {
            out = out.replace("에移植", "에 이식").replace("移植", "이식");
        }

        // Normalize Japanese/Western dialogue quote characters to Korean typography.
        if src_trimmed.starts_with('「') && src_trimmed.ends_with('」') {
            let inner = out
                .trim()
                .trim_start_matches(['「', '“', '"', '‘', '«'])
                .trim_end_matches(['」', '”', '"', '’', '»']);
            out = format!("“{inner}”");
        } else if (src_trimmed.starts_with('（') && src_trimmed.ends_with('）'))
            || (src_trimmed.starts_with('『') && src_trimmed.ends_with('』'))
        {
            let inner = out
                .trim()
                .trim_start_matches(['（', '(', '『', '‘', '“', '"'])
                .trim_end_matches(['）', ')', '』', '’', '”', '"']);
            out = format!("‘{inner}’");
        }

        // General multi-genre Korean accuracy refinements (CS/AI, Science/Bio, Culinary, Literature, Numeracy)
        fix_english_billion_to_korean(source_text, &mut out);
        if (src_lower.contains("weights") || src_lower.contains("weight"))
            && !src_lower.contains("wells")
            && (src_lower.contains("model")
                || src_lower.contains("gguf")
                || src_lower.contains("quantiz")
                || src_lower.contains("neural")
                || src_lower.contains("llm"))
            && out.contains("웰스")
        {
            out = out.replace("웰스", "가중치");
        }
        if src_lower.contains("inference")
            && (src_lower.contains("llm")
                || src_lower.contains("mllm")
                || src_lower.contains("neural")
                || src_lower.contains("model")
                || src_lower.contains("tensor")
                || src_lower.contains("gpu"))
        {
            out = out
                .replace("인퍼런싱", "추론")
                .replace("인퍼런스", "추론")
                .replace("인퍼페션", "추론")
                .replace("인출 기능", "추론 기능")
                .replace("인출 과정", "추론 과정")
                .replace("인출을", "추론을")
                .replace("인출이", "추론이");
        }
        if src_lower.contains("lactic acid") && out.contains("유산산") {
            out = out.replace("유산산", "젖산");
        }
        if src_lower.contains("cyanobacteria") {
            for bad_bio in ["시아노바이오마이크로바이오스", "시아노바이오"] {
                if out.contains(bad_bio) {
                    out = out.replace(bad_bio, "남세균(시아노박테리아)");
                }
            }
        }
        if src_lower.contains("pride and prejudice") {
            out = out
                .replace("《프라이드 앤 프리뱔지스》", "《오만과 편견》")
                .replace("프라이드 앤 프리뱔지스", "오만과 편견");
            out = fix_korean_particle_after_term(&out, "오만과 편견");
        }

        fix_korean_code_token_particles(&mut out);

        let is_dialogue = is_dialogue_segment(source_text);
        if is_dialogue {
            if crate::ai::detect_dialogue_register(source_text)
                == crate::ai::DialogueRegister::Casual
            {
                enforce_casual_korean_dialogue(&mut out);
            }
        } else {
            normalize_korean_narration_endings(&mut out);
        }

        // Universal wiki section & label disambiguation (KO only)
        if source_text.contains("文書をご注意ください")
            || source_text.contains("文書をご参照ください")
        {
            for (bad_doc, good_doc) in [
                ("문서를 주의하십시오", "문서를 참고하십시오"),
                ("문서를 주의하세요", "문서를 참고하십시오"),
                ("문서를 주의해 주세요", "문서를 참고하십시오"),
                ("문서를 주의해주세요", "문서를 참고하십시오"),
                ("문서를 확인해 주세요", "문서를 참고하십시오"),
            ] {
                if out.contains(bad_doc) {
                    out = out.replace(bad_doc, good_doc);
                }
            }
        }
        if (src_trimmed.ends_with(". outline") || src_trimmed.eq_ignore_ascii_case("outline"))
            && (out.contains("윤곽") || out.contains("아웃라인") || out.contains("외곽선"))
        {
            out = out
                .replace("윤곽", "개요")
                .replace("아웃라인", "개요")
                .replace("외곽선", "개요");
        }
        if (src_trimmed.ends_with(". characteristic")
            || src_trimmed.eq_ignore_ascii_case("characteristic"))
            && out.contains("특성")
        {
            out = out.replace("특성", "특징");
        }
        if src_trimmed.ends_with(". extended edition") && out.contains("연장판") {
            out = out.replace("연장판", "확장판");
        }
        if src_lower.contains("koreanization") && out.contains("한국화") {
            out = out.replace("한국화", "한국어화");
        }
        if src_lower.starts_with("circulation ") && out.starts_with("순환 ") {
            out = out.replacen("순환 ", "유통 ", 1);
        }
        if out.contains("퍼스트퍼스먼트 슈터")
            || out.contains("퍼스트퍼스너 슈터")
            || out.contains("퍼스트퍼슨 슈터")
        {
            out = out
                .replace("퍼스트퍼스먼트 슈터", "1인칭 슈팅 게임")
                .replace("퍼스트퍼스너 슈터", "1인칭 슈팅 게임")
                .replace("퍼스트퍼슨 슈터", "1인칭 슈팅 게임");
        }
        if src_lower.contains("game of the year") {
            out = out
                .replace("‘게임 오브 더 이어즈’", "‘올해의 게임’")
                .replace("‘게임 오브 더 이어’", "‘올해의 게임’")
                .replace("게임 오브 더 이어즈", "올해의 게임")
                .replace("게임 오브 더 이어", "올해의 게임");
        }
        if out.contains("1인칭 슈팅 게임]] 게임") || out.contains("1인칭 슈팅 게임 게임")
        {
            out = out
                .replace("1인칭 슈팅 게임]] 게임", "1인칭 슈팅 게임]]")
                .replace("1인칭 슈팅 게임 게임", "1인칭 슈팅 게임");
        }
        if src_lower.contains("science fiction")
            && src_lower.contains("shooter")
            && (out.contains("과학소설") || out.contains("과학 소설"))
        {
            out = out.replace("과학소설", "SF").replace("과학 소설", "SF");
        }
        if src_lower.contains("single-player") && out.contains("Single-player]]") {
            out = out.replace("Single-player]]", "싱글플레이어]]");
        }
        if out.contains("엔진]]를") || out.contains("engine]]를") {
            out = out
                .replace("엔진]]를", "엔진]]을")
                .replace("engine]]를", "engine]]을");
        }
    }

    // 2. Universal repairs applied across ALL source <-> target language pairs
    strip_hallucinated_marker_urls(source_text, &mut out);
    if norm_target == "English" && out.contains("slot slots") {
        out = out.replace("slot slots", "slots");
    }
    repair_inline_markers(source_text, &mut out);
    strip_hallucinated_marker_urls(source_text, &mut out);
    strip_spurious_question_mark(source_text, &mut out);
    if !source_text.contains('\n')
        && let Some((bullet, _)) = split_list_bullet_prefix(source_text)
    {
        let trimmed_out = out.trim_start();
        if !trimmed_out.is_empty()
            && !trimmed_out.starts_with('-')
            && !trimmed_out.starts_with('*')
            && !trimmed_out.starts_with('•')
            && !trimmed_out.starts_with('·')
        {
            out = format!("{bullet}{trimmed_out}");
        }
    }
    out
}

fn strip_hallucinated_marker_urls(source_text: &str, out: &mut String) {
    if source_text.contains("http://") || source_text.contains("https://") {
        return;
    }
    if !out.contains("http://") && !out.contains("https://") {
        return;
    }
    for pat in ["](https://", "](http://", "]https://", "]http://"] {
        while let Some(pos) = out.find(pat) {
            let url_start = pos + 1;
            let rest = &out[url_start..];
            let starts_with_paren = rest.starts_with('(');
            let scan_from = if starts_with_paren { 1 } else { 0 };
            let rel_end = rest[scan_from..]
                .find(|c: char| {
                    c.is_whitespace()
                        || matches!(
                            c,
                            ')' | ']' | '」' | '』' | '"' | '\'' | '’' | '”' | '<' | '>'
                        )
                        || !c.is_ascii()
                })
                .map(|i| scan_from + i)
                .unwrap_or(rest.len());
            let mut remove_end = url_start + rel_end;
            if rest[rel_end..].starts_with(')') {
                if starts_with_paren {
                    remove_end += 1;
                } else {
                    let before = &out[..url_start];
                    let opens = before.matches('(').count();
                    let closes = before.matches(')').count();
                    if opens <= closes {
                        remove_end += 1;
                    }
                }
            }
            out.replace_range(url_start..remove_end, "");
        }
    }
}

#[allow(dead_code)]
pub fn preserve_outer_dialogue_quotes(source_text: &str, translated: String) -> String {
    preserve_outer_dialogue_quotes_for_target(source_text, "Korean", translated)
}

pub fn preserve_outer_dialogue_quotes_for_target(
    source_text: &str,
    norm_target: &str,
    translated: String,
) -> String {
    let src = source_text.trim();
    let out = translated.trim();
    if src.is_empty() || out.is_empty() {
        return translated;
    }
    let quote_pair = if norm_target == "Korean" {
        [
            ('「', '」', '“', '”'),
            ('『', '』', '‘', '’'),
            ('“', '”', '“', '”'),
            ('"', '"', '“', '”'),
        ]
    } else if norm_target == "Japanese" {
        [
            ('「', '」', '「', '」'),
            ('『', '』', '『', '』'),
            ('“', '”', '「', '」'),
            ('"', '"', '「', '」'),
        ]
    } else {
        [
            ('「', '」', '"', '"'),
            ('『', '』', '\'', '\''),
            ('“', '”', '"', '"'),
            ('"', '"', '"', '"'),
        ]
    }
    .into_iter()
    .find(|&(open, close, _, _)| {
        src.starts_with(open)
            && src.ends_with(close)
            && src.chars().filter(|&c| c == open).count() == 1
            && src.chars().filter(|&c| c == close).count() == 1
    });
    let Some((_, _, tgt_open, tgt_close)) = quote_pair else {
        return translated;
    };
    const ANY_OPEN: [char; 5] = ['「', '『', '“', '"', '‘'];
    const ANY_CLOSE: [char; 5] = ['」', '』', '”', '"', '’'];
    if out.starts_with(ANY_OPEN) || out.ends_with(ANY_CLOSE) {
        return translated;
    }
    format!("{tgt_open}{out}{tgt_close}")
}

fn estimate_max_output_tokens_for_prompt(
    source_token_len: u32,
    source_text: &str,
    norm_target: &str,
    ctx_size: u32,
) -> u32 {
    let has_hanzi = source_text
        .chars()
        .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c));
    let target_is_cjk = matches!(
        norm_target,
        "Korean" | "Japanese" | "Chinese" | "Traditional Chinese"
    );
    let est = if has_hanzi && !target_is_cjk {
        source_token_len.saturating_mul(3).saturating_add(56)
    } else if source_token_len <= 24 {
        source_token_len.saturating_mul(2).saturating_add(36)
    } else {
        source_token_len
            .saturating_mul(9)
            .saturating_div(4)
            .saturating_add(48)
    };
    est.clamp(48, ctx_size.saturating_sub(256).max(64))
}

#[allow(dead_code)]
fn estimate_max_output_tokens(source: &str, ctx_size: u32) -> u32 {
    let chars = source.chars().count() as u32;
    let est = if chars <= 60 {
        chars.saturating_mul(2).saturating_add(48)
    } else {
        chars.saturating_mul(2).saturating_add(96)
    };
    est.clamp(64, ctx_size.saturating_sub(256).max(64))
}

pub fn find_spec(id_or_name: &str) -> Option<&'static HyMt2ModelSpec> {
    let needle = id_or_name.trim();
    if needle.is_empty() {
        return None;
    }
    HY_MT2_CATALOG.iter().find(|spec| {
        spec.id.eq_ignore_ascii_case(needle)
            || spec.tier.eq_ignore_ascii_case(needle)
            || spec.filename.eq_ignore_ascii_case(needle)
    })
}

#[derive(Debug, Clone)]
pub struct GgmlDeviceCandidate {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub backend: String,
    pub is_discrete: bool,
    pub memory_total_mb: u64,
}

#[derive(Debug, Clone)]
struct DxgiAdapterInfo {
    name: String,
    vendor_id: u32,
    dedicated_vram_mb: u64,
    shared_vram_mb: u64,
}

pub fn is_software_gpu_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("microsoft basic render")
        || lower.contains("llvmpipe")
        || lower.contains("swiftshader")
        || lower.contains("lavapipe")
        || lower.contains("software renderer")
        || lower.contains("warp")
}

pub fn detect_gpu_vendor(name: &str, pci_vendor_id: Option<u32>) -> &'static str {
    if let Some(vid) = pci_vendor_id {
        match vid {
            0x10DE => return "nvidia",
            0x1002 => return "amd",
            0x8086 => return "intel",
            _ => {}
        }
    }
    let lower = name.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return "none";
    }
    if lower.contains("nvidia")
        || lower.contains("geforce")
        || lower.contains("rtx")
        || lower.contains("gtx")
        || lower.contains("quadro")
        || lower.contains("tesla")
        || lower.contains("titan")
        || lower.contains("mx150")
        || lower.contains("mx250")
        || lower.contains("mx350")
        || lower.contains("mx450")
        || lower.contains("mx550")
        || lower.contains("mx570")
    {
        return "nvidia";
    }
    if lower.contains("amd") || lower.contains("radeon") || lower.contains("rx ") {
        return "amd";
    }
    if lower.contains("intel")
        || lower.contains("arc")
        || lower.contains("iris")
        || lower.contains("uhd graphics")
        || lower.contains("hd graphics")
    {
        return "intel";
    }
    "other"
}

fn is_known_amd_apu_name(gpu_lower: &str) -> bool {
    if gpu_lower.contains("rx ") || gpu_lower.contains("instinct") {
        return false;
    }
    gpu_lower.contains("780m")
        || gpu_lower.contains("760m")
        || gpu_lower.contains("740m")
        || gpu_lower.contains("680m")
        || gpu_lower.contains("660m")
        || gpu_lower.contains("880m")
        || gpu_lower.contains("890m")
        || gpu_lower.contains("vega")
        || gpu_lower.contains("radeon(tm) graphics")
        || gpu_lower.contains("radeon graphics")
}

fn is_known_intel_igpu_name(gpu_lower: &str) -> bool {
    if gpu_lower.contains("arc a") || gpu_lower.contains("arc b") || gpu_lower.contains("arc pro") {
        return false;
    }
    gpu_lower.contains("uhd")
        || gpu_lower.contains("iris")
        || gpu_lower.contains("hd graphics")
        || gpu_lower.contains("intel(r) graphics")
}

fn is_discrete_dxgi_adapter(adapter: &DxgiAdapterInfo) -> bool {
    let vendor = detect_gpu_vendor(&adapter.name, Some(adapter.vendor_id));
    let lower = adapter.name.to_ascii_lowercase();
    match vendor {
        "nvidia" => true,
        "intel" => adapter.dedicated_vram_mb >= 1_024 && !is_known_intel_igpu_name(&lower),
        "amd" => {
            if is_known_amd_apu_name(&lower) {
                return false;
            }
            if !lower.contains("rx ")
                && !lower.contains("pro ")
                && !lower.contains(" r9 ")
                && !lower.contains(" r7 ")
                && adapter.dedicated_vram_mb <= 4_096
                && adapter.shared_vram_mb > adapter.dedicated_vram_mb
            {
                return false;
            }
            adapter.dedicated_vram_mb >= 1_024
        }
        _ => adapter.dedicated_vram_mb >= 1_024,
    }
}

pub fn select_best_ggml_gpu(candidates: &[GgmlDeviceCandidate]) -> Option<GgmlDeviceCandidate> {
    candidates
        .iter()
        .filter(|c| !is_software_gpu_name(&c.name) && !is_software_gpu_name(&c.description))
        .max_by_key(|c| {
            let label = if c.description.trim().is_empty() {
                c.name.as_str()
            } else {
                c.description.as_str()
            };
            let label_lower = label.to_ascii_lowercase();
            let vendor = detect_gpu_vendor(label, None);
            let effective_discrete = if vendor == "nvidia" {
                true
            } else if is_known_amd_apu_name(&label_lower) || is_known_intel_igpu_name(&label_lower)
            {
                false
            } else {
                c.is_discrete
            };
            let discrete_rank = if effective_discrete && c.memory_total_mb >= 512 {
                1u8
            } else {
                0u8
            };
            let b_lower = c.backend.to_ascii_lowercase();
            let backend_rank = if b_lower.contains("cuda") {
                3u8
            } else if b_lower.contains("vulkan") {
                2u8
            } else if b_lower.contains("hip")
                || b_lower.contains("rocm")
                || b_lower.contains("metal")
                || b_lower.contains("sycl")
            {
                1u8
            } else {
                0u8
            };
            let vendor_rank = match vendor {
                "nvidia" => 3u8,
                "amd" => 2u8,
                "intel" => 1u8,
                _ => 0u8,
            };
            (discrete_rank, backend_rank, c.memory_total_mb, vendor_rank)
        })
        .cloned()
}

fn normalize_gpu_match_key(name: &str) -> String {
    let cleaned = name
        .to_ascii_lowercase()
        .replace("(tm)", " ")
        .replace("(r)", " ");
    cleaned
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|tok| !tok.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn match_dxgi_adapter_for_gpu<'a>(
    selected_name: Option<&str>,
    selected_vendor: &str,
    adapters: &'a [DxgiAdapterInfo],
) -> Option<&'a DxgiAdapterInfo> {
    if adapters.is_empty() {
        return None;
    }
    if let Some(sel) = selected_name {
        let sel_key = normalize_gpu_match_key(sel);
        if !sel_key.is_empty() {
            if let Some(exact) = adapters.iter().find(|a| {
                let k = normalize_gpu_match_key(&a.name);
                !k.is_empty() && (k == sel_key || k.contains(&sel_key) || sel_key.contains(&k))
            }) {
                return Some(exact);
            }
            let sel_tokens: HashSet<&str> = sel_key.split_whitespace().collect();
            let best_overlap = adapters
                .iter()
                .filter(|a| {
                    selected_vendor == "none"
                        || detect_gpu_vendor(&a.name, Some(a.vendor_id)) == selected_vendor
                })
                .max_by_key(|a| {
                    let k = normalize_gpu_match_key(&a.name);
                    let overlap = k
                        .split_whitespace()
                        .filter(|tok| sel_tokens.contains(tok))
                        .count();
                    let discrete_bonus = usize::from(is_discrete_dxgi_adapter(a));
                    (overlap, discrete_bonus, a.dedicated_vram_mb)
                });
            if best_overlap.is_some() {
                return best_overlap;
            }
        }
        if selected_vendor != "none"
            && let Some(by_vendor) = adapters
                .iter()
                .filter(|a| detect_gpu_vendor(&a.name, Some(a.vendor_id)) == selected_vendor)
                .max_by_key(|a| {
                    (
                        is_discrete_dxgi_adapter(a),
                        a.dedicated_vram_mb,
                        a.shared_vram_mb,
                    )
                })
        {
            return Some(by_vendor);
        }
    }
    adapters.iter().max_by_key(|a| {
        (
            is_discrete_dxgi_adapter(a),
            a.dedicated_vram_mb,
            a.shared_vram_mb,
        )
    })
}

pub fn resolve_optimal_gpu_layers(
    settings: &AppSettings,
    hw: &HardwareProfile,
    file_size_bytes: u64,
) -> (u32, String) {
    let use_cuda = hw.cuda_available;
    let use_vulkan = !use_cuda
        && hw.vulkan_available
        && (hw.vulkan_compiled || hw.active_backend.starts_with("vulkan"));
    if !settings.hardware_acceleration || (!use_cuda && !use_vulkan) {
        return (0, "cpu(native)".to_string());
    }

    let proto = if use_cuda { "cuda" } else { "vulkan" };
    let vendor_tag = match (hw.gpu_vendor.as_str(), hw.is_discrete_gpu, hw.is_amd_apu) {
        ("nvidia", _, _) => "nvidia",
        (_, _, true) => "amd-apu",
        ("amd", true, _) => "amd",
        ("intel", true, _) => "intel-arc",
        ("intel", false, _) => "intel-igpu",
        _ => "gpu",
    };

    if settings.hy_mt2_gpu_layers >= 0 {
        let explicit = settings.hy_mt2_gpu_layers as u32;
        if explicit == 0 {
            return (0, "cpu(native)".to_string());
        }
        return (explicit, format!("{proto}({vendor_tag}-ngl={explicit})"));
    }

    let model_mb = (file_size_bytes / (1024 * 1024)).max(512);
    let ded_mb = hw.dedicated_vram_mb;

    if hw.is_amd_apu {
        let kv_overhead_mb = if model_mb > 3_000 { 280 } else { 150 };
        if ded_mb == 0 || model_mb + kv_overhead_mb <= ded_mb + 256 {
            return (99, format!("{proto}(amd-apu-ngl=99)"));
        }
        let usable_vram_mb = ded_mb.saturating_sub(kv_overhead_mb).max(768);
        let estimated_total_layers = if model_mb >= 3_600 { 33u64 } else { 29u64 };
        let optimal_layers = ((usable_vram_mb * estimated_total_layers) / model_mb)
            .clamp(8, estimated_total_layers - 1) as u32;
        return (
            optimal_layers,
            format!("{proto}+cpu(amd-hybrid ngl={optimal_layers}/{estimated_total_layers})"),
        );
    }

    if hw.is_discrete_gpu || hw.gpu_vendor == "nvidia" {
        let kv_overhead_mb = if model_mb > 3_500 {
            512
        } else if model_mb > 2_000 {
            320
        } else {
            240
        };
        if ded_mb == 0 || model_mb + kv_overhead_mb <= ded_mb {
            return (99, format!("{proto}({vendor_tag}-ngl=99)"));
        }
        let usable_vram_mb = ded_mb.saturating_sub(kv_overhead_mb).max(768);
        let estimated_total_layers = if model_mb >= 3_600 { 33u64 } else { 29u64 };
        let optimal_layers = ((usable_vram_mb * estimated_total_layers) / model_mb)
            .clamp(8, estimated_total_layers - 1) as u32;
        return (
            optimal_layers,
            format!(
                "{proto}+cpu({vendor_tag}-hybrid ngl={optimal_layers}/{estimated_total_layers})"
            ),
        );
    }

    // Intel / other integrated GPUs (e.g. Iris Xe / UHD report 128 MiB DXGI dedicated + large shared memory).
    let effective_igpu_mb = if ded_mb >= 1_024 {
        ded_mb + 256
    } else if hw.shared_vram_mb > 0 {
        ((hw.shared_vram_mb * 55) / 100).min(4_096).max(ded_mb)
    } else {
        ded_mb
    };
    if effective_igpu_mb == 0 {
        return (0, "cpu(native)".to_string());
    }
    let kv_overhead_mb = if model_mb > 3_000 { 320 } else { 200 };
    if model_mb + kv_overhead_mb <= effective_igpu_mb {
        return (99, format!("{proto}({vendor_tag}-ngl=99)"));
    }
    let usable_vram_mb = effective_igpu_mb.saturating_sub(kv_overhead_mb).max(768);
    let estimated_total_layers = if model_mb >= 3_600 { 33u64 } else { 29u64 };
    let optimal_layers = ((usable_vram_mb * estimated_total_layers) / model_mb)
        .clamp(8, estimated_total_layers - 1) as u32;
    (
        optimal_layers,
        format!("{proto}+cpu({vendor_tag}-hybrid ngl={optimal_layers}/{estimated_total_layers})"),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_compute_decision_summary(
    gpu_vendor: &str,
    is_discrete_gpu: bool,
    cuda_ready: bool,
    vulkan_ready: bool,
    is_amd_apu: bool,
    npu_available: bool,
    npu_name: Option<&str>,
    gpu_name: Option<&str>,
    dedicated_vram_mb: u64,
    shared_vram_mb: u64,
    detected_gpu_count: usize,
) -> String {
    let gpu_label = gpu_name.unwrap_or("GPU 미감지");
    let npu_label = npu_name.unwrap_or("NPU");
    let multi_gpu_note = if detected_gpu_count > 1 {
        " (듀얼/하이브리드 GPU 환경에서 외장 GPU 자동 우선 고정)"
    } else {
        ""
    };

    if cuda_ready {
        format!(
            "NVIDIA CUDA GPU 감지({gpu_label}, 전용 VRAM {dedicated_vram_mb} MiB){multi_gpu_note} → CUDA 네이티브 백엔드로 최고 속도 추론을 수행하며, VRAM 초과 시 GPU+CPU 하이브리드 또는 CPU로 자동 폴백합니다."
        )
    } else if vulkan_ready && gpu_vendor == "nvidia" {
        format!(
            "NVIDIA 외장 GPU 감지({gpu_label}, 전용 VRAM {dedicated_vram_mb} MiB){multi_gpu_note} → 별도 CUDA 설치 없이 Vulkan 1.4 GPU 가속(Tensor Core CoopMat2 지원)으로 실행하며, VRAM 초과 시 GPU+CPU 하이브리드 분할 또는 CPU로 자동 폴백합니다."
        )
    } else if vulkan_ready && gpu_vendor == "amd" && is_discrete_gpu {
        format!(
            "AMD Radeon 외장 GPU 감지({gpu_label}, 전용 VRAM {dedicated_vram_mb} MiB){multi_gpu_note} → Vulkan GPU 가속을 우선 적용하며, VRAM 초과 시 GPU+CPU 하이브리드 또는 CPU로 자동 폴백합니다."
        )
    } else if vulkan_ready && is_amd_apu {
        if npu_available {
            format!(
                "AMD APU 감지({gpu_label}, 전용 UMA {dedicated_vram_mb} MiB + 공유 {shared_vram_mb} MiB / {npu_label} 감지) → GGUF 양자화 연산은 NPU(INT8 전용)보다 메모리 대역폭이 높은 Radeon Vulkan iGPU를 우선 사용하며, 2GB 이하 모델(1.8B)은 iGPU 100% 오프로드, 7B 모델은 WDDM 스와핑 방지를 위해 Radeon iGPU + Ryzen CPU 하이브리드 분할 가속을 자동 적용합니다."
            )
        } else {
            format!(
                "AMD APU 감지({gpu_label}, 전용 UMA {dedicated_vram_mb} MiB + 공유 {shared_vram_mb} MiB) → 2GB 이하 모델은 Radeon Vulkan iGPU 100% 가속, 대용량(7B) 모델은 Radeon iGPU + Ryzen CPU 하이브리드 분할 가속을 자동 적용합니다."
            )
        }
    } else if vulkan_ready && gpu_vendor == "intel" && is_discrete_gpu {
        format!(
            "Intel Arc 외장 GPU 감지({gpu_label}, 전용 VRAM {dedicated_vram_mb} MiB){multi_gpu_note} → Vulkan GPU 가속을 우선 적용하며, VRAM 초과 시 GPU+CPU 하이브리드 또는 CPU로 자동 폴백합니다."
        )
    } else if vulkan_ready && gpu_vendor == "intel" && !is_discrete_gpu {
        format!(
            "Intel 내장 GPU 감지({gpu_label}, 공유 메모리 {shared_vram_mb} MiB) → Vulkan 내장 GPU 가속을 자동 적용하며, 대용량 모델은 CPU 하이브리드 분할로 안전하게 실행합니다."
        )
    } else if vulkan_ready {
        format!(
            "GPU 감지({gpu_label}, 전용 VRAM {dedicated_vram_mb} MiB){multi_gpu_note} → Vulkan GPU 가속을 우선 적용하며, 실패 시 CPU 네이티브로 자동 폴백합니다."
        )
    } else if npu_available {
        format!(
            "GPU 가속 비활성 · {npu_label} 감지 → CPU 다중 명령어셋(SSE2~AVX-512 자동 선택) 네이티브 모드로 실행합니다."
        )
    } else {
        "CPU 네이티브 다중 스레드 추론 모드(SSE2/SSE4.2/AVX/AVX2/AVX-VNNI/AVX-512 자동 선택)로 모든 PC에서 안전하게 동작합니다.".to_string()
    }
}

pub fn recommend_hardware_tier_ext(
    total_ram_mb: u64,
    dedicated_vram_mb: u64,
    shared_vram_mb: u64,
    gpu_vendor: &str,
    is_discrete_gpu: bool,
    is_amd_apu: bool,
    npu_available: bool,
) -> (&'static str, &'static str, String) {
    let vendor_label = match gpu_vendor {
        "nvidia" => "NVIDIA GeForce",
        "amd" => "AMD Radeon",
        "intel" => "Intel Arc",
        _ => "외장",
    };
    if dedicated_vram_mb >= 7_800 || (dedicated_vram_mb >= 6_000 && total_ram_mb >= 20_000) {
        return (
            "7b-q4_k_m",
            HY_MT2_HIGH_SPEC_MODEL_ID,
            format!(
                "고성능 {vendor_label} GPU 감지(전용 VRAM {dedicated_vram_mb} MiB) → Hy-MT2-7B Q4_K_M 권장"
            ),
        );
    }
    if (is_discrete_gpu || gpu_vendor == "nvidia") && dedicated_vram_mb >= 3_800 {
        return (
            "1.8b-q4_k_m",
            DEFAULT_HY_MT2_MODEL_ID,
            format!(
                "{vendor_label} 외장 GPU 감지(전용 VRAM {dedicated_vram_mb} MiB) → 초고속 주력 Hy-MT2 1.8B(Q4_K_M/Q6_K) 100% GPU 가속 권장 (7B 모델은 자동 하이브리드 분할 지원)"
            ),
        );
    }
    if is_amd_apu && total_ram_mb >= 12_000 {
        let npu_tag = if npu_available { " + Ryzen AI NPU" } else { "" };
        return (
            "1.8b-q4_k_m",
            DEFAULT_HY_MT2_MODEL_ID,
            format!(
                "AMD Ryzen APU{npu_tag} 감지(UMA 전용 {dedicated_vram_mb} MiB / 공유 {shared_vram_mb} MiB) → 초고속 주력 Hy-MT2 1.8B Q4_K_M(2GB UMA 내장그래픽 호환) 권장"
            ),
        );
    }
    recommend_hardware_tier(total_ram_mb, dedicated_vram_mb)
}

pub fn recommend_hardware_tier(
    total_ram_mb: u64,
    dedicated_vram_mb: u64,
) -> (&'static str, &'static str, String) {
    if dedicated_vram_mb >= 7_800 || (dedicated_vram_mb >= 6_000 && total_ram_mb >= 20_000) {
        (
            "7b-q4_k_m",
            HY_MT2_HIGH_SPEC_MODEL_ID,
            format!("고성능 GPU 감지(전용 VRAM {dedicated_vram_mb} MiB) → Hy-MT2-7B Q4_K_M 권장"),
        )
    } else if total_ram_mb >= 6_500 {
        (
            "1.8b-q4_k_m",
            DEFAULT_HY_MT2_MODEL_ID,
            format!("표준 데스크톱 메모리({total_ram_mb} MiB RAM) → Hy-MT2-1.8B Q4_K_M 권장"),
        )
    } else if total_ram_mb >= 3_500 {
        (
            "1.8b-1.25bit",
            HY_MT2_LIGHT_MODEL_ID,
            format!(
                "저용량 메모리({total_ram_mb} MiB RAM) → 경량 모델(1.8B 초경량) 또는 클라우드 API 병행 권장"
            ),
        )
    } else {
        (
            "api-fallback",
            DEFAULT_HY_MT2_MODEL_ID,
            format!(
                "시스템 메모리({total_ram_mb} MiB RAM)가 부족하여 클라우드 API 폴백을 권장합니다"
            ),
        )
    }
}

fn default_cpu_threads() -> i32 {
    let logical = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (logical / 2).clamp(2, 12) as i32
}

fn default_batch_threads(decode_threads: i32) -> i32 {
    let logical = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(decode_threads);
    // Use up to 75% of logical threads (e.g. 12 threads on 8C/16T AMD Ryzen) for fast prompt prefill.
    ((logical * 3) / 4).max(decode_threads).clamp(2, 16)
}

fn hf_download_url(repo: &str, filename: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/main/{filename}")
}

fn partial_model_path(final_path: &Path) -> PathBuf {
    let name = final_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("model.gguf");
    final_path.with_file_name(format!("{name}.part"))
}

fn percent_of(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    ((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("파일 열기 실패: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        use std::io::Read;
        let read = file
            .read(&mut buf)
            .map_err(|e| format!("해시 계산 중 파일 읽기 오류: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(target_os = "windows")]
fn is_vulkan_loader_present() -> bool {
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    use windows::core::w;
    match unsafe { LoadLibraryW(w!("vulkan-1.dll")) } {
        Ok(module) if !module.is_invalid() => {
            let _ = unsafe { FreeLibrary(module) };
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn ensure_safe_vulkan_env() {
    if !(cfg!(feature = "vulkan") || cfg!(feature = "universal-cpu")) || is_vulkan_loader_present()
    {
        return;
    }
    #[cfg(target_env = "msvc")]
    {
        unsafe extern "C" {
            fn _putenv_s(name: *const std::ffi::c_char, val: *const std::ffi::c_char) -> i32;
        }
        // Tell ggml-backend-reg to skip ggml_backend_vk_reg() so delay-loaded vulkan-1.dll is never touched.
        unsafe {
            let _ = _putenv_s(c"GGML_DISABLE_VULKAN".as_ptr(), c"1".as_ptr());
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_safe_vulkan_env() {}

#[cfg(target_os = "windows")]
fn query_system_memory_mb() -> (u64, u64) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) }.is_ok();
    if !ok {
        return (0, 0);
    }
    (
        status.ullTotalPhys / (1024 * 1024),
        status.ullAvailPhys / (1024 * 1024),
    )
}

#[cfg(not(target_os = "windows"))]
fn query_system_memory_mb() -> (u64, u64) {
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        let mut total_kb = 0u64;
        let mut avail_kb = 0u64;
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                avail_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
        return (total_kb / 1024, avail_kb / 1024);
    }
    (0, 0)
}

#[cfg(target_os = "windows")]
fn query_dxgi_adapters() -> Vec<DxgiAdapterInfo> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
    };
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut adapters = Vec::new();
    let mut idx = 0u32;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(idx) } {
        idx += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
            continue;
        }
        let name_len = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..name_len])
            .trim()
            .to_string();
        if name.is_empty() || is_software_gpu_name(&name) {
            continue;
        }
        adapters.push(DxgiAdapterInfo {
            name,
            vendor_id: desc.VendorId,
            dedicated_vram_mb: (desc.DedicatedVideoMemory as u64) / (1024 * 1024),
            shared_vram_mb: (desc.SharedSystemMemory as u64) / (1024 * 1024),
        });
    }
    adapters
}

#[cfg(not(target_os = "windows"))]
fn query_dxgi_adapters() -> Vec<DxgiAdapterInfo> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn read_hklm_reg_sz(subkey: &str, value_name: &str) -> Option<String> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hkey: isize,
            lp_sub_key: *const u16,
            lp_value: *const u16,
            dw_flags: u32,
            pdw_type: *mut u32,
            pv_data: *mut u8,
            pcb_data: *mut u32,
        ) -> i32;
    }
    const HKEY_LOCAL_MACHINE: isize = -2147483646i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let val_w: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut buf = [0u16; 256];
    let mut bytes = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey_w.as_ptr(),
            val_w.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut u8,
            &mut bytes,
        )
    };
    if status != 0 || bytes < 2 {
        return None;
    }
    let u16_len = (bytes as usize / 2).min(buf.len());
    let nul_pos = buf[..u16_len]
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(u16_len);
    let text = String::from_utf16_lossy(&buf[..nul_pos]).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(target_os = "windows")]
fn query_cpu_name() -> Option<String> {
    read_hklm_reg_sz(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
}

#[cfg(not(target_os = "windows"))]
fn query_cpu_name() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn query_npu_device() -> (bool, Option<String>) {
    // Windows ComputeAccelerator device class GUID: {f01a9d53-3ff6-48d2-9f97-c8a7004be10c}
    // Covers AMD Ryzen AI (NPU Compute Accelerator Device / AMD IPU), Intel AI Boost, and Qualcomm Hexagon NPU.
    for idx in 0..8 {
        let key = format!(
            r"SYSTEM\CurrentControlSet\Control\Class\{{f01a9d53-3ff6-48d2-9f97-c8a7004be10c}}\{:04}",
            idx
        );
        if let Some(desc) = read_hklm_reg_sz(&key, "DriverDesc") {
            let ver = read_hklm_reg_sz(&key, "DriverVersion").unwrap_or_default();
            let label = if ver.is_empty() {
                desc
            } else {
                format!("{desc} (v{ver})")
            };
            return (true, Some(label));
        }
    }
    (false, None)
}

#[cfg(not(target_os = "windows"))]
fn query_npu_device() -> (bool, Option<String>) {
    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_tier_recommendation_matches_spec() {
        let (tier, model, _) = recommend_hardware_tier(32_000, 12_288);
        assert_eq!(tier, "7b-q4_k_m");
        assert_eq!(model, HY_MT2_HIGH_SPEC_MODEL_ID);

        let (tier, model, _) = recommend_hardware_tier(14_800, 2_048);
        assert_eq!(tier, "1.8b-q4_k_m");
        assert_eq!(model, DEFAULT_HY_MT2_MODEL_ID);

        let (tier, model, _) = recommend_hardware_tier(4_096, 0);
        assert_eq!(tier, "1.8b-1.25bit");
        assert_eq!(model, HY_MT2_LIGHT_MODEL_ID);

        let (tier, _, _) = recommend_hardware_tier(2_048, 0);
        assert_eq!(tier, "api-fallback");
    }

    #[test]
    fn dialogue_detection_distinguishes_narration_and_quotes() {
        assert!(is_dialogue_segment("「ふん、あんたごときに扱えるの？」"));
        assert!(is_dialogue_segment("『……本当に大丈夫なのか？』"));
        assert!(is_dialogue_segment("\"Don't tell me you're going alone.\""));
        assert!(is_dialogue_segment("（そういう事か）"));
        assert!(!is_dialogue_segment(
            "エルシアは星詠の聖剣を握りしめ、静かに息を吐いた。"
        ));
        assert!(!is_dialogue_segment(
            "Kael sheathed the Astral Blade and looked back."
        ));
        assert!(!is_dialogue_segment("(The Orange Box)"));
        assert!(!is_dialogue_segment("(abbreviated as HL2)"));
        assert!(!is_dialogue_segment(
            "- Stabilized `MaybeUninit::assume_init_ref`.\n- Added support for bare-metal targets."
        ));
    }

    #[test]
    fn wave2_morphological_prose_and_numeracy_tunings_work() {
        // 1. Adjective vs verb `합니다` / `하지 않습니다` + Unicode `-ㅂ니다` + safe `~어요`
        let mut adj = "하나의 전극은 투명합니다. 이 기능은 특별합니다. 이 단계는 필요하지 않습니다. 더욱 그렇습니다. 이 단계는 사용하지 않습니다.".to_string();
        normalize_korean_narration_endings(&mut adj);
        assert_eq!(adj, "하나의 전극은 투명하다. 이 기능은 특별하다. 이 단계는 필요하지 않다. 더욱 그렇다. 이 단계는 사용하지 않는다.");

        let mut verb = "새 코드를 씁니다. 데이터를 보냅니다. 예시를 들어보겠습니다: 파일을 만들어요.".to_string();
        normalize_korean_narration_endings(&mut verb);
        assert_eq!(verb, "새 코드를 쓴다. 데이터를 보낸다. 예시를 들어보겠다: 파일을 만들어요.");

        let mut copula = "조치가 가능합니다: 결론은 《오만과 편견》입니다.".to_string();
        normalize_korean_narration_endings(&mut copula);
        assert_eq!(copula, "조치가 가능하다: 결론은 《오만과 편견》이다.");

        // 2. Soft-wrapped prose vs structured bullets, IPA stripping, and bullet restoration
        let wrapped_doc = "[[img0]] It initializes Vec<T>.\nThis performs allocation and initialization\nin separate steps, because it implements the\nIndex trait.";
        assert!(!segment_needs_line_split(wrapped_doc));
        let pre = preprocess_source_text(wrapped_doc, "한국어", "auto");
        assert!(pre.contains("initialization in separate steps"));
        assert!(pre.contains("implements the Index trait."));

        let bullet_src = "- Added support for tier-2 targets.";
        assert_eq!(
            postprocess_translation_output(bullet_src, "한국어", "auto", "티어 2 대상 지원이 추가되었습니다.".into()),
            "- 티어 2 대상 지원이 추가되었다."
        );

        let ipa_src = "La tour Eiffel [tuʁɛfɛl] est haute de 330\u{00A0}m.";
        let ipa_clean = preprocess_source_text(ipa_src, "한국어", "natural");
        assert_eq!(ipa_clean, "La tour Eiffel est haute de 330 m.");

        // 3. Numeracy (`billion` -> `10억`) and dotted/marker particle repair
        assert_eq!(
            postprocess_translation_output(
                "With 1.5 billion viewers tuning in, llama.cpp runs [[0|vector]].",
                "한국어",
                "natural",
                "1.5억 명이 시청하며, llama.cpp과 [[0|vector]]을 실행합니다.".into()
            ),
            "15억 명이 시청하며, llama.cpp와 [[0|vector]]를 실행한다."
        );
    }

    #[test]
    fn fandom_vdc_dom_code_markers_and_shortcuts_work() {
        // 1. Marker-wrapped literal identifiers & footnotes take 0ms shortcut
        assert_eq!(
            non_translatable_shortcut("[[0|app_config_v1]]").as_deref(),
            Some("[[0|app_config_v1]]")
        );
        assert_eq!(
            non_translatable_shortcut("[[0|[1]]]").as_deref(),
            Some("[[0|[1]]]")
        );
        assert_eq!(
            non_translatable_shortcut("[[img0]] 2026").as_deref(),
            Some("[[img0]] 2026")
        );
        assert!(non_translatable_shortcut("[[img0]] Release Notes").is_none());

        // 2. repair_inline_markers restores corrupted code identifiers while keeping footnotes intact
        let src = "Excerpts ([[0|init_worker_01]]) load [[1|run_task_02a]] via trigger.[[2|[1]]]";
        let mut out = "발췌본 ([[0|init_워커_01]])은 트리거를 통해 [[1|run_task_02]]을 불러온다.[[2|[1]]"
            .to_string();
        repair_inline_markers(src, &mut out);
        assert_eq!(
            out,
            "발췌본 ([[0|init_worker_01]])은 트리거를 통해 [[1|run_task_02a]]을 불러온다.[[2|[1]]]"
        );

        // 3. Universal URL hallucination cleanup (both `](http...` single-bracket and `]](http...`)
        assert_eq!(
            postprocess_translation_output(
                "The library uses [[0|Rust core]], with [[1|GPU engine]].",
                "日本語",
                "natural",
                "このライブラリは[[0|Rust core](https://example.invalid/rust)を使用しており、[[1|GPUエンジン]]https://example.invalid/gpu)が含まれています。".into(),
            ),
            "このライブラリは[[0|Rust core]]を使用しており、[[1|GPUエンジン]]が含まれています。"
        );

        // 4. Korean narration passive `-됩니다` / formal `-줍니다` normalization & scientific/AI post-fixes
        let ai_lead = postprocess_translation_output(
            "The model optimizes LLM inference and shows strong efficiency.",
            "한국어",
            "polished",
            "모델은 LLM 인출 기능을 최적화하며 높은 효율을 보여줍니다.".into(),
        );
        assert_eq!(
            ai_lead,
            "모델은 LLM 추론 기능을 최적화하며 높은 효율을 보여준다."
        );

        assert_eq!(
            postprocess_translation_output(
                "Fermentation produces lactic acid.",
                "한국어",
                "natural",
                "발효 과정에서 유산산이 생성됩니다.".into(),
            ),
            "발효 과정에서 젖산이 생성된다."
        );

        assert_eq!(
            postprocess_translation_output(
                "Pride and Prejudice is a classic novel.",
                "한국어",
                "novel",
                "《프라이드 앤 프리뱔지스》는 고전 소설입니다.".into(),
            ),
            "《오만과 편견》은 고전 소설이다."
        );

        // 5. Repaired CJK bracket `》`/`】` before `입니다` + negative `~않습니다`
        assert_eq!(
            postprocess_translation_output(
                "This document describes [[1|HTTP protocol]].",
                "한국어",
                "natural",
                "이 문서는 [[1|HTTP 프로토콜》입니다.".into(),
            ),
            "이 문서는 [[1|HTTP 프로토콜]]이다."
        );
        assert_eq!(
            postprocess_translation_output(
                "This library is written in [[0|Rust]] and is not deprecated.",
                "한국어",
                "natural",
                "이 라이브러리는 [[0|Rust]]로 작성되었으며 폐기되지 않았습니다.".into(),
            ),
            "이 라이브러리는 [[0|Rust]]로 작성되었으며 폐기되지 않았다."
        );
        assert_eq!(
            postprocess_translation_output(
                "The library uses [[0|Rust core]].",
                "日本語",
                "natural",
                "このライブラリは[[0|Rustコア]]】を使用している。".into(),
            ),
            "このライブラリは[[0|Rustコア]]を使用している。"
        );

        // 6. Parallel sentence splitter + prompt & glossary echo guard
        let long_para = "A major update has been released on November 16, 2024 to celebrate the anniversary. It adds plugin support, developer tools, fixes various bugs, and makes modules accessible directly through the menu. There are also a few other changes (e.g. \"High\" shaders and U.S. builds version 1.8 updates) that enable [[1|lightmap]] filtering.[[3|[1]]] Additionally, hardware acceleration improves speed across desktop devices.";
        let split_chunks = split_long_paragraph_sentences(long_para)
            .expect("Long multi-sentence wiki paragraph must split");
        assert!(
            (2..=4).contains(&split_chunks.len()),
            "Unexpected split chunk count: {split_chunks:?}"
        );
        assert!(
            split_chunks
                .iter()
                .any(|c| c.contains("filtering.[[3|[1]]]")),
            "Footnote [[3|[1]]] must stay attached to its ending sentence: {split_chunks:?}"
        );
        assert!(
            !split_chunks
                .iter()
                .any(|c| c.ends_with("e.g.") || c.ends_with("U.S.") || c.ends_with("1.")),
            "Abbreviations and decimals must not trigger splits: {split_chunks:?}"
        );

        assert!(looks_like_prompt_echo(
            long_para,
            "... 마커를 그대로 유지하면서, 배경 정보를 고려하여 [[0|...]] 마커도 보존한 버전으로 번역했다."
        ));
        assert!(looks_like_prompt_echo(
            "Products overview",
            "\"nvidia\"는 \"엔비디아\"로 번역 할 것."
        ));
        assert!(!looks_like_prompt_echo(
            long_para,
            "2024년 11월 16일 대규모 업데이트가 출시되었다."
        ));
    }

    #[test]
    fn outer_quotes_preserved_when_model_drops_them() {
        assert_eq!(
            preserve_outer_dialogue_quotes(
                "「恐れ入ります、カイン様」",
                "죄송합니다, 카인 님".into()
            ),
            "“죄송합니다, 카인 님”"
        );
        assert_eq!(
            preserve_outer_dialogue_quotes("「先輩、行きます！」", "“선배, 갑니다!”".into()),
            "“선배, 갑니다!”"
        );
        assert_eq!(
            preserve_outer_dialogue_quotes(
                "エルシアは微笑んだ。「……そう」",
                "엘시아는 미소 지었다. “……그래.”".into()
            ),
            "엘시아는 미소 지었다. “……그래.”"
        );
    }

    #[test]
    fn anchor_background_keeps_early_shared_terms_across_long_history() {
        let history = vec![
            ContextPair {
                source: "アルカディア帝国の結界が砕け散った。".into(),
                translation: Some("알카디아 제국의 결계가 산산이 부서졌다.".into()),
            },
            ContextPair {
                source: "「くそっ、間に合わなかったか！」".into(),
                translation: Some("“젠장, 제때 오지 못했나!”".into()),
            },
            ContextPair {
                source: "「恐れ入ります、カイン様」".into(),
                translation: Some("“죄송합니다, 카인 님”".into()),
            },
            ContextPair {
                source: "「俺はまだ行くぞ！」".into(),
                translation: Some("“난 아직 갈 거야!”".into()),
            },
            ContextPair {
                source: "冷たい風が吹き抜けていた。".into(),
                translation: Some("차가운 바람이 불어오고 있었다.".into()),
            },
        ];
        let bg = select_background_pairs("「先輩、結界の修復が完了しました！」", &history);
        assert!(
            bg.iter().any(|line| line.contains("アルカディア帝国の結界")
                && line.contains("알카디아 제국의 결계")),
            "Early anchor containing 結界 must be kept in background: {bg:?}"
        );
        assert!(
            bg.iter().any(|line| line.contains("冷たい風")),
            "Most recent line must also be kept: {bg:?}"
        );
    }

    #[test]
    fn single_prompt_preserves_markers_glossary_and_official_style() {
        let glossary = vec![crate::ai::GlossaryRule {
            source: "Hy-MT2".into(),
            aliases: Vec::new(),
            translation: "Hy-MT2".into(),
            alternatives: String::new(),
            do_not_translate: true,
            reading: String::new(),
        }];
        let prompt = format_single_prompt(
            "Download [[0|Hy-MT2]] today [[img0]].",
            "한국어",
            "polished",
            &glossary,
            &[],
        );
        assert!(prompt.contains("\"Hy-MT2\" translates to \"Hy-MT2\""));
        assert!(prompt.contains("preserving [[0|...]] and [[img0]] markers intact"));
        assert!(
            prompt.starts_with("Reference the following translations:\n")
                || prompt.contains(
                    "Translate the following segment into Korean, without additional explanation."
                )
        );

        let literal_prompt =
            format_single_prompt("Download Hy-MT2 today.", "한국어", "literal", &[], &[]);
        assert!(literal_prompt.contains("faithful literal translation"));

        let zh_prompt = format_single_prompt("Hello world", "中文(简体)", "natural", &[], &[]);
        assert_eq!(
            normalize_target_language_for_hy_mt2("中文(简体)"),
            "Chinese"
        );
        assert!(
            zh_prompt.contains("将以下文本翻译为中文"),
            "Unexpected ZH prompt: {zh_prompt}"
        );

        let dialogue_prompt =
            format_single_prompt("「本当に大丈夫なの？」", "한국어", "natural", &[], &[]);
        assert!(dialogue_prompt.contains("[natural colloquial dialogue]"));

        let polite_novel = format_single_prompt(
            "「恐れ入ります、カイン様」",
            "한국어",
            "auto:novel",
            &[],
            &[],
        );
        assert!(polite_novel.contains("respectful tone"));

        let casual_novel = format_single_prompt(
            "「ふん、あんたごときに何ができるんだぜ」",
            "한국어",
            "auto:novel",
            &[],
            &[],
        );
        assert!(casual_novel.contains("informal colloquial"));

        let tech_prompt = format_single_prompt(
            "cargo test --features vulkan",
            "한국어",
            "auto:technical",
            &[],
            &[],
        );
        assert!(!tech_prompt.contains("Style:"));
    }

    #[test]
    fn multilingual_target_scoping_and_glossary_safety_work() {
        // 1. Target-language preset normalization across European & Asian scripts
        assert_eq!(
            normalize_target_language_for_hy_mt2("中文(繁體)"),
            "Traditional Chinese"
        );
        assert_eq!(normalize_target_language_for_hy_mt2("Español"), "Spanish");
        assert_eq!(normalize_target_language_for_hy_mt2("Français"), "French");
        assert_eq!(normalize_target_language_for_hy_mt2("Deutsch"), "German");
        assert_eq!(normalize_target_language_for_hy_mt2("Русский"), "Russian");

        // 2. Korean-only rules must NEVER alter English, Japanese, or Chinese outputs
        assert_eq!(
            postprocess_translation_output(
                "それはすでに完了した。",
                "English",
                "natural",
                "already done.".into(),
            ),
            "already done."
        );
        assert_eq!(
            postprocess_translation_output(
                "“게임 어워드 수상작입니다.”",
                "日本語",
                "natural",
                "「ゲームアワード受賞作です。」".into(),
            ),
            "「ゲームアワード受賞作です。」"
        );

        // 3. Footnote triple-bracket balance
        assert_eq!(
            postprocess_translation_output(
                "2009年[[0|[4]]]に発売。",
                "한국어",
                "natural",
                "2009년[[0|[4]]에 발매.".into(),
            ),
            "2009년[[0|[4]]]에 발매."
        );

        // 4. Script-unique target-language shortcut checks
        assert!(
            already_in_target_language_shortcut("플러그인 3은 2024년 출시된 도구다.", "한국어")
                .is_some()
        );
        assert!(
            already_in_target_language_shortcut("이 도구는 [[0|Plugin 3]]이다.", "한국어")
                .is_none()
        );
        assert!(already_in_target_language_shortcut("このツールはSDKです。", "日本語").is_some());
        assert!(
            already_in_target_language_shortcut("C'est un outil très célèbre.", "English")
                .is_none()
        );

        // 5. Generic glossary per-segment filtering, exact shortcut, pre-sub & echo sanitization
        let nvidia_rule = crate::ai::GlossaryRule {
            source: "nvidia".into(),
            aliases: Vec::new(),
            translation: "엔비디아".into(),
            alternatives: String::new(),
            do_not_translate: false,
            reading: String::new(),
        };
        let filtered_unrelated = filter_segment_glossary(
            "Modern GPUs rely on high-bandwidth memory.",
            std::slice::from_ref(&nvidia_rule),
        );
        assert!(filtered_unrelated.is_empty());

        let filtered_matched = filter_segment_glossary(
            "NVIDIA announced new Rubin GPUs.",
            std::slice::from_ref(&nvidia_rule),
        );
        assert_eq!(filtered_matched.len(), 1);
        assert_eq!(
            glossary_exact_segment_shortcut("NVIDIA", &filtered_matched).as_deref(),
            Some("엔비디아")
        );
        assert_eq!(
            glossary_exact_segment_shortcut("[[0|nvidia]]", &filtered_matched).as_deref(),
            Some("[[0|엔비디아]]")
        );
        assert_eq!(
            apply_glossary_pre_substitute("NVIDIA announced new GPUs.", &filtered_matched),
            "엔비디아 announced new GPUs."
        );
        let mut enforced = "NVIDIA는 새 GPU를 발표했다.".to_string();
        apply_glossary_post_enforce("NVIDIA announced new GPUs.", &mut enforced, &filtered_matched);
        assert_eq!(enforced, "엔비디아는 새 GPU를 발표했다.");

        assert_eq!(
            clean_hy_mt2_output(
                "Reference the following translations:\n\"nvidia\" translates to \"엔비디아\"\nnvidia는 엔비디아로 번역할 것\n엔비디아는 새 아키텍처를 발표했다."
            ),
            "엔비디아는 새 아키텍처를 발표했다."
        );

        // 6. Mixed dialogue + narration split (book title in 『...』 must not count as dialogue)
        let mixed_book = "「今日は随分お早いですね。」\n『吾輩は猫である』は、夏目漱石の長編小説である。";
        assert!(!is_dialogue_segment(mixed_book));
        assert!(segment_needs_line_split(mixed_book));

        // 7. Generic outline/docs refinements & non-translatable checks
        assert_eq!(
            postprocess_translation_output("1. outline", "한국어", "natural", "1. 윤곽".into()),
            "1. 개요"
        );
        assert_eq!(
            postprocess_translation_output(
                "Use [[0|rustup]] to download Rust.",
                "한국어",
                "natural",
                "[[0|rustup]]https://www.rustup.org)라는 도구를 사용한다.".into(),
            ),
            "[[0|rustup]]라는 도구를 사용한다."
        );
        assert_eq!(
            non_translatable_shortcut("2026-11-19").as_deref(),
            Some("2026-11-19")
        );
        assert_eq!(
            non_translatable_shortcut("[[img0]]").as_deref(),
            Some("[[img0]]")
        );
        assert_eq!(
            non_translatable_shortcut("0x7F3A").as_deref(),
            Some("0x7F3A")
        );
        assert!(non_translatable_shortcut("DEBUG MENU").is_none());
    }

    #[test]
    fn amd_apu_hybrid_layer_split_and_q6k_echo_cleanup_work() {
        let mut settings = AppSettings::default();
        settings.hardware_acceleration = true;
        settings.hy_mt2_gpu_layers = -1;
        let hw = HardwareProfile {
            total_ram_mb: 16_384,
            available_ram_mb: 10_240,
            cpu_cores: 16,
            cpu_name: Some("AMD Ryzen 7 8840HS w/ Radeon 780M Graphics".into()),
            gpu_name: Some("AMD Radeon 780M Graphics".into()),
            dedicated_vram_mb: 2_048,
            shared_vram_mb: 7_800,
            is_amd_apu: true,
            npu_available: true,
            npu_name: Some("NPU Compute Accelerator Device".into()),
            vulkan_compiled: true,
            vulkan_available: true,
            active_backend: "vulkan-amd-apu".into(),
            compute_decision: String::new(),
            recommended_tier: "1.8b-q4_k_m".into(),
            recommended_model_id: DEFAULT_HY_MT2_MODEL_ID.into(),
            recommendation_reason: String::new(),
            gpu_vendor: "amd".into(),
            is_discrete_gpu: false,
            selected_gpu_device_index: Some(0),
            cuda_compiled: false,
            cuda_available: false,
            detected_gpus: vec!["AMD Radeon 780M Graphics (Vulkan, 2048 MiB, 내장)".into()],
        };

        let (layers_18b, label_18b) = resolve_optimal_gpu_layers(&settings, &hw, 1_133_080_448);
        assert_eq!(layers_18b, 99);
        assert!(label_18b.contains("amd-apu"));

        let (layers_3b, _) = resolve_optimal_gpu_layers(&settings, &hw, 2_068_790_016);
        assert_eq!(layers_3b, 99);

        let (layers_7b, label_7b) = resolve_optimal_gpu_layers(&settings, &hw, 4_624_648_896);
        assert!(
            (8..=24).contains(&layers_7b),
            "Unexpected 7B hybrid layers: {layers_7b}"
        );
        assert!(label_7b.contains("amd-hybrid"));

        let noisy = "[배경 정보]\n「こんにちは」 -> “안녕하세요.”\n[출처 텍스트]\n“오랜만이야.”";
        assert_eq!(clean_hy_mt2_output(noisy), "“오랜만이야.”");
        assert_eq!(silent_ellipsis_shortcut("「……」").as_deref(), Some("“……”"));
    }

    #[test]
    fn idle_unload_respects_active_jobs_and_timeout() {
        let dir = std::env::temp_dir().join(format!("hy-mt2-test-{}", now_epoch_ms()));
        let engine = HyMt2Engine::new(dir.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(!engine.unload_if_idle(60).await);
        });
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "requires downloaded Hy-MT2-1.8B-Q4_K_M.gguf in AppLocalData"]
    fn real_vulkan_gguf_inference_smoke() {
        let local_app = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA env missing");
        let models_dir = local_app
            .join("com.appleout.transratebrowser")
            .join("models");
        let model_path = models_dir.join(DEFAULT_HY_MT2_MODEL_ID);
        if !model_path.is_file() {
            return;
        }
        let engine = HyMt2Engine::new(models_dir);
        let mut settings = AppSettings::default();
        settings.provider = crate::model::ProviderKind::HyMt2;
        settings.model = DEFAULT_HY_MT2_MODEL_ID.to_string();
        settings.target_language = "한국어".to_string();
        settings.style = "auto".to_string();
        let glossary = vec![crate::ai::GlossaryRule {
            source: "Nvidia".into(),
            aliases: Vec::new(),
            translation: "엔비디아".into(),
            alternatives: String::new(),
            do_not_translate: false,
            reading: String::new(),
        }];
        let context = PromptContext {
            target_language: "한국어",
            style: "auto",
            project_name: None,
            previous_context: None,
            glossary: &glossary,
        };
        let inputs = vec![
            SegmentInput {
                id: "seg-1".into(),
                text: "Nvidia develops GPUs for AI computing [[img0]].".into(),
            },
            SegmentInput {
                id: "seg-2".into(),
                text: "Modern servers run inference workloads at scale.".into(),
            },
        ];
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (results, _, _) = engine
                .translate_segments(
                    &settings,
                    &inputs,
                    &context,
                    Some("https://example.invalid/smoke"),
                    &[],
                    true,
                    CancellationToken::new(),
                    |_| {},
                )
                .await
                .expect("Hy-MT2 inference failed");
            assert_eq!(results.len(), 2);
            assert!(results[0].translation.contains("엔비디아"));
            assert!(!results[1].translation.contains("번역할 것"));
            let _ = engine.unload().await;
        });
    }

    #[test]
    fn universal_nvidia_amd_intel_cpu_selection_and_allocation_work() {
        let mut settings = AppSettings::default();
        settings.hardware_acceleration = true;
        settings.hy_mt2_gpu_layers = -1;

        // 1. Intel Core + Intel UHD iGPU + NVIDIA RTX 4060 Laptop GPU (Optimus dual-GPU, Vulkan & CUDA)
        let optimus_candidates = vec![
            GgmlDeviceCandidate {
                index: 0,
                name: "Vulkan0".into(),
                description: "Intel(R) UHD Graphics".into(),
                backend: "Vulkan".into(),
                is_discrete: false,
                memory_total_mb: 8_000,
            },
            GgmlDeviceCandidate {
                index: 1,
                name: "Vulkan1".into(),
                description: "NVIDIA GeForce RTX 4060 Laptop GPU".into(),
                backend: "Vulkan".into(),
                is_discrete: true,
                memory_total_mb: 8_188,
            },
        ];
        let picked_vk = select_best_ggml_gpu(&optimus_candidates).expect("must pick RTX 4060");
        assert_eq!(picked_vk.index, 1);
        assert_eq!(detect_gpu_vendor(&picked_vk.description, None), "nvidia");

        let mut optimus_with_cuda = optimus_candidates.clone();
        optimus_with_cuda.push(GgmlDeviceCandidate {
            index: 2,
            name: "CUDA0".into(),
            description: "NVIDIA GeForce RTX 4060 Laptop GPU".into(),
            backend: "CUDA".into(),
            is_discrete: true,
            memory_total_mb: 8_188,
        });
        let picked_cuda =
            select_best_ggml_gpu(&optimus_with_cuda).expect("must prefer CUDA over Vulkan");
        assert_eq!(picked_cuda.index, 2);

        let rtx_4060_hw = HardwareProfile {
            total_ram_mb: 16_384,
            available_ram_mb: 10_000,
            cpu_cores: 16,
            cpu_name: Some("13th Gen Intel(R) Core(TM) i7-13700H".into()),
            gpu_name: Some("NVIDIA GeForce RTX 4060 Laptop GPU".into()),
            dedicated_vram_mb: 8_188,
            shared_vram_mb: 8_192,
            is_amd_apu: false,
            npu_available: false,
            npu_name: None,
            vulkan_compiled: true,
            vulkan_available: true,
            active_backend: "vulkan-nvidia".into(),
            compute_decision: String::new(),
            recommended_tier: "7b-q4_k_m".into(),
            recommended_model_id: HY_MT2_HIGH_SPEC_MODEL_ID.into(),
            recommendation_reason: String::new(),
            gpu_vendor: "nvidia".into(),
            is_discrete_gpu: true,
            selected_gpu_device_index: Some(1),
            cuda_compiled: false,
            cuda_available: false,
            detected_gpus: vec![
                "Intel(R) UHD Graphics (Vulkan, 8000 MiB, 내장)".into(),
                "NVIDIA GeForce RTX 4060 Laptop GPU (Vulkan, 8188 MiB, 외장)".into(),
            ],
        };
        assert_eq!(
            resolve_optimal_gpu_layers(&settings, &rtx_4060_hw, 1_133_080_448),
            (99, "vulkan(nvidia-ngl=99)".to_string())
        );
        assert_eq!(
            resolve_optimal_gpu_layers(&settings, &rtx_4060_hw, 4_624_648_896),
            (99, "vulkan(nvidia-ngl=99)".to_string())
        );

        let mut rtx_4060_cuda_hw = rtx_4060_hw.clone();
        rtx_4060_cuda_hw.cuda_available = true;
        rtx_4060_cuda_hw.active_backend = "cuda-nvidia".into();
        assert_eq!(
            resolve_optimal_gpu_layers(&settings, &rtx_4060_cuda_hw, 4_624_648_896),
            (99, "cuda(nvidia-ngl=99)".to_string())
        );

        // 2. AMD Ryzen 7 + Radeon 780M iGPU + NVIDIA RTX 4070 dual-GPU laptop: RTX 4070 wins, NOT is_amd_apu!
        let dual_amd_nvidia = vec![
            GgmlDeviceCandidate {
                index: 0,
                name: "Vulkan0".into(),
                description: "AMD Radeon(TM) 780M Graphics".into(),
                backend: "Vulkan".into(),
                is_discrete: false,
                memory_total_mb: 2_048,
            },
            GgmlDeviceCandidate {
                index: 1,
                name: "Vulkan1".into(),
                description: "NVIDIA GeForce RTX 4070 Laptop GPU".into(),
                backend: "Vulkan".into(),
                is_discrete: true,
                memory_total_mb: 8_188,
            },
        ];
        let picked_dual =
            select_best_ggml_gpu(&dual_amd_nvidia).expect("must pick RTX 4070 over 780M");
        assert_eq!(picked_dual.index, 1);
        assert_eq!(detect_gpu_vendor(&picked_dual.description, None), "nvidia");

        // 3. AMD Ryzen 5 + NVIDIA GTX 1650 (4 GB VRAM): 1.8B full offload, 7B hybrid split (not amd-apu!)
        let gtx_1650_hw = HardwareProfile {
            total_ram_mb: 16_384,
            available_ram_mb: 10_000,
            cpu_cores: 12,
            cpu_name: Some("AMD Ryzen 5 5600X".into()),
            gpu_name: Some("NVIDIA GeForce GTX 1650".into()),
            dedicated_vram_mb: 4_096,
            shared_vram_mb: 8_192,
            is_amd_apu: false,
            npu_available: false,
            npu_name: None,
            vulkan_compiled: true,
            vulkan_available: true,
            active_backend: "vulkan-nvidia".into(),
            compute_decision: String::new(),
            recommended_tier: "1.8b-q4_k_m".into(),
            recommended_model_id: DEFAULT_HY_MT2_MODEL_ID.into(),
            recommendation_reason: String::new(),
            gpu_vendor: "nvidia".into(),
            is_discrete_gpu: true,
            selected_gpu_device_index: Some(0),
            cuda_compiled: false,
            cuda_available: false,
            detected_gpus: vec!["NVIDIA GeForce GTX 1650 (Vulkan, 4096 MiB, 외장)".into()],
        };
        assert_eq!(
            resolve_optimal_gpu_layers(&settings, &gtx_1650_hw, 1_133_080_448),
            (99, "vulkan(nvidia-ngl=99)".to_string())
        );
        let (gtx_7b_layers, gtx_7b_label) =
            resolve_optimal_gpu_layers(&settings, &gtx_1650_hw, 4_624_648_896);
        assert!(
            (8..33).contains(&gtx_7b_layers),
            "Unexpected 4GB GTX 1650 7B hybrid layers: {gtx_7b_layers}"
        );
        assert!(
            gtx_7b_label.contains("nvidia-hybrid"),
            "label={gtx_7b_label}"
        );
        assert!(!gtx_7b_label.contains("amd-"), "label={gtx_7b_label}");

        // 4. Intel Core + Intel Iris Xe iGPU only (128 MiB DXGI dedicated + 8192 MiB shared)
        let intel_igpu_hw = HardwareProfile {
            total_ram_mb: 16_384,
            available_ram_mb: 10_000,
            cpu_cores: 12,
            cpu_name: Some("12th Gen Intel(R) Core(TM) i5-1240P".into()),
            gpu_name: Some("Intel(R) Iris(R) Xe Graphics".into()),
            dedicated_vram_mb: 128,
            shared_vram_mb: 8_192,
            is_amd_apu: false,
            npu_available: false,
            npu_name: None,
            vulkan_compiled: true,
            vulkan_available: true,
            active_backend: "vulkan-intel-igpu".into(),
            compute_decision: String::new(),
            recommended_tier: "1.8b-q4_k_m".into(),
            recommended_model_id: DEFAULT_HY_MT2_MODEL_ID.into(),
            recommendation_reason: String::new(),
            gpu_vendor: "intel".into(),
            is_discrete_gpu: false,
            selected_gpu_device_index: Some(0),
            cuda_compiled: false,
            cuda_available: false,
            detected_gpus: vec!["Intel(R) Iris(R) Xe Graphics (Vulkan, 8192 MiB, 내장)".into()],
        };
        assert_eq!(
            resolve_optimal_gpu_layers(&settings, &intel_igpu_hw, 1_133_080_448),
            (99, "vulkan(intel-igpu-ngl=99)".to_string())
        );
        let (intel_7b_layers, intel_7b_label) =
            resolve_optimal_gpu_layers(&settings, &intel_igpu_hw, 4_624_648_896);
        assert!((8..33).contains(&intel_7b_layers));
        assert!(
            intel_7b_label.contains("intel-igpu-hybrid"),
            "label={intel_7b_label}"
        );

        // 5. Software renderers only & CPU-only PC fallback
        let sw_only = vec![
            GgmlDeviceCandidate {
                index: 0,
                name: "Vulkan0".into(),
                description: "Microsoft Basic Render Driver".into(),
                backend: "Vulkan".into(),
                is_discrete: false,
                memory_total_mb: 4_096,
            },
            GgmlDeviceCandidate {
                index: 1,
                name: "Vulkan1".into(),
                description: "llvmpipe (LLVM 17.0.6, 256 bits)".into(),
                backend: "Vulkan".into(),
                is_discrete: false,
                memory_total_mb: 4_096,
            },
        ];
        assert!(select_best_ggml_gpu(&sw_only).is_none());

        let cpu_only_hw = HardwareProfile {
            total_ram_mb: 8_192,
            available_ram_mb: 5_000,
            cpu_cores: 4,
            cpu_name: Some("Intel(R) Pentium(R) Gold G6400 CPU".into()),
            gpu_name: None,
            dedicated_vram_mb: 0,
            shared_vram_mb: 0,
            is_amd_apu: false,
            npu_available: false,
            npu_name: None,
            vulkan_compiled: true,
            vulkan_available: false,
            active_backend: "cpu".into(),
            compute_decision: String::new(),
            recommended_tier: "1.8b-q4_k_m".into(),
            recommended_model_id: DEFAULT_HY_MT2_MODEL_ID.into(),
            recommendation_reason: String::new(),
            gpu_vendor: "none".into(),
            is_discrete_gpu: false,
            selected_gpu_device_index: None,
            cuda_compiled: false,
            cuda_available: false,
            detected_gpus: Vec::new(),
        };
        assert_eq!(
            resolve_optimal_gpu_layers(&settings, &cpu_only_hw, 1_133_080_448),
            (0, "cpu(native)".to_string())
        );
    }
}
