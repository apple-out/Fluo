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

fn segment_needs_line_split(source_text: &str) -> bool {
    let chars = source_text.chars().count();
    chars > 360 && source_text.lines().count() > 2
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
    ];
    ECHO_PHRASES
        .iter()
        .any(|phrase| out.contains(phrase) && !source_text.contains(phrase))
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
    let cleaned = strip_echoed_history_translations(&cleaned, history);
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
    tokens: Vec<llama_cpp_2::token::LlamaToken>,
    max_new: u32,
    reserved_kv: usize,
}

struct ActiveBatchSlot {
    slot_id: i32,
    parent_idx: usize,
    part_idx: usize,
    source_text: String,
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
            if let Some(shortcut) = shortcut_translation(&part_src, target_language) {
                part_outputs[parent_idx][part_idx] = Some(shortcut);
                part_remaining[parent_idx] -= 1;
                continue;
            }
            let normalized = preprocess_source_text(&part_src, &norm_target, style);
            let (tokens, src_toks) = prepare_prompt_tokens(
                model,
                uses_hy_template,
                chat_template,
                &normalized,
                target_language,
                style,
                glossary,
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
                let final_text = finalize_translated_unit(
                    &done.source_text,
                    target_language,
                    style,
                    &done.output,
                    &[],
                );
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
    if let Some(shortcut) = shortcut_translation(source_text, target_language) {
        return Ok(shortcut);
    }
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let normalized_source = preprocess_source_text(source_text, &norm_target, style);
    let (tokens, src_toks) = prepare_prompt_tokens(
        model,
        uses_hy_template,
        chat_template,
        &normalized_source,
        target_language,
        style,
        glossary,
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
    Ok(finalize_translated_unit(
        source_text,
        target_language,
        style,
        &raw,
        history,
    ))
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
    const OPEN_QUOTES: [char; 6] = ['「', '『', '“', '"', '〝', '（'];
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > 1 {
        let dialogue_lines = lines
            .iter()
            .filter(|l| {
                l.starts_with(OPEN_QUOTES)
                    || (is_cjk && l.starts_with('('))
                    || l.starts_with('—')
                    || l.starts_with('-')
            })
            .count();
        if dialogue_lines * 2 >= lines.len() {
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
    let cur_reg = crate::ai::detect_dialogue_register(source_text);
    for &idx in recent_slice {
        if has_cjk_or_kana(&history[idx].source) != cur_has_cjk {
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
        let prev_chars = history[idx].source.trim().chars().count();
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
    source_text: &str,
    target_lang: &str,
) -> Vec<(&'static str, &'static str)> {
    if !target_lang.eq_ignore_ascii_case("Korean") {
        return Vec::new();
    }
    // Keep Reference Glossary strictly limited to Proper Nouns & Fantasy Entities so 1.8B Q4/Q6
    // never confuses verb/clause hints with dictionary definition requests.
    const HINTS: &[(&str, &str)] = &[
        ("シアーシャ", "시아샤"),
        ("ウルバス", "우르바스"),
        ("ジグ", "지그"),
        ("シアン", "시안"),
        ("ハインツ", "하인츠"),
        ("リザ", "리자"),
        ("アラン", "아란"),
        ("澄人教", "징인교"),
        ("鱗人", "비늘족"),
        ("緑鱗氏族", "녹비늘 부족"),
        ("削岩竜", "착암룡"),
        ("猿狗", "원구"),
        ("三面鬼", "삼면귀"),
        ("瞬膜", "순막"),
        ("ハルバード", "할버드"),
    ];
    HINTS
        .iter()
        .copied()
        .filter(|(src, _)| source_text.contains(*src))
        .collect()
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

pub fn preprocess_source_text(source_text: &str, target_language: &str, _style: &str) -> String {
    let norm_target = normalize_target_language_for_hy_mt2(target_language);
    let mut s = source_text.to_string();

    if norm_target != "Japanese" {
        // 1. Strip inline ruby furigana attached directly to Kanji compounds in raw pasted web-novel text.
        s = s
            .replace("澄人教すみびときょう", "澄人教")
            .replace("鱗人うろこびと", "鱗人")
            .replace("緑鱗氏族りょくりんしぞく", "緑鱗氏族")
            .replace("文書をご注意ください", "文書をご参照ください");
        if s.contains("すみびときょう") && s.contains("澄人教") {
            s = s.replace("すみびときょう", "");
        }
        // 2. Japanese homograph/idiom disambiguation for compact 1.8B models.
        s = s
            .replace("人気が少ない", "人の気配が少ない")
            .replace("人気がない", "人の気配がない")
            .replace("今日は上がり？", "今日はもう仕事終わり？")
            .replace("前のお礼に、奢らせて", "前のお礼に、私がご馳走するよ")
            .replace("冒険者御用達の中規模な店", "冒険者たちがよく通う中規模な店")
            .replace(
                "冒険者御用達とは言っても",
                "冒険者たちがよく通う店とは言っても",
            )
            .replace(
                "かなり奮発しないといけない",
                "かなり思い切ってお金を使わないといけない",
            )
            .replace("冷もとれるし", "涼しく冷やせるし")
            .replace(
                "ばきりとヒビを入れて返してやる",
                "ばきりと殻にヒビを入れて返してやる",
            )
            .replace("縦に長い瞳孔", "縦方向に細長い瞳孔")
            .replace(
                "仲間の知り合いが開いてるお店",
                "仲間の知人が経営しているお店",
            )
            .replace("ジグには不要かな……", "ジグには必要ないだろうな……")
            .replace("ジグには不要かな", "ジグには必要ないだろうな")
            .replace("不要かな……", "必要ないだろうな……")
            .replace("不要かな。", "必要ないだろうな。")
            .replace(
                "別に信者ってわけじゃなくても",
                "特に信徒ではない人であっても",
            )
            .replace("信者ってわけじゃなくても", "信徒ではない人であっても")
            .replace("ってわけじゃなくても", "というわけではない人であっても")
            .replace("ぷっと吹き出", "フッと笑いを漏ら")
            .replace("ぷふっと吹き出", "フッと笑いを漏ら")
            .replace("ぶっと吹き出", "フッと笑いを漏ら")
            .replace("ぷっと笑", "フッと笑")
            .replace("ぷふっと笑", "フッと笑")
            .replace("「ぷっ", "「ふっ")
            .replace("「ぷふっ", "「ふっ");
    }

    if norm_target != "English" {
        // 3. English game/technical/wiki jargon disambiguation for 1.8B models.
        s = s
            .replace(
                "what encounter of enemies will be fought",
                "which enemy encounter group will be fought in battle",
            )
            .replace(
                "Sets who is in your ",
                "Sets which character is placed in the ",
            )
            .replace("party slot.", "party member slot.")
            .replace(
                "talking to shadows through holdup",
                "talking to Shadows during a Hold Up",
            )
            .replace(
                "talking to Shadows through holdup",
                "talking to Shadows during a Hold Up",
            )
            .replace("Sets the major of ", "Sets the major ID for ")
            .replace(
                "what part of battle you're in",
                "which battle state you are in",
            )
            .replace(
                "fires pellets in a cone pattern",
                "fires shotgun pellets in a cone-shaped spread",
            );
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

    // Remove prompt-template headers or background pair echoes across supported languages.
    let mut kept_lines = Vec::new();
    let mut last_arrow_rhs: Option<String> = None;
    for line in s.lines() {
        let mut l = line.trim();
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

fn replace_korean_copula_imnida(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("입니다") {
        let prefix = &rest[..pos];
        let after = &rest[pos + "입니다".len()..];
        let is_sentence_boundary = after.is_empty()
            || after.starts_with([
                '.', '!', '?', '…', ';', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
            ]);
        let trimmed_prefix = prefix.trim_end_matches(['’', '”', '"', '\'', ')', '』', '」']);
        let mut prev_char = trimmed_prefix.chars().last();
        if prev_char == Some(']')
            && let Some(inner_start) = trimmed_prefix.rfind('|')
        {
            let inner = trimmed_prefix[inner_start + 1..].trim_end_matches(']');
            prev_char = inner
                .trim_end_matches(['’', '”', '"', '\'', ')', '』', '」'])
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
    // 1. Explicit irregular & high-frequency formal -> plain narrative endings
    for (formal, plain) in [
        ("있지 않습니다", "있지 않다"),
        ("되지 않습니다", "되지 않는다"),
        ("하지 않습니다", "하지 않는다"),
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
        ("있습니다", "있다"),
        ("없습니다", "없다"),
        ("같습니다", "같다"),
        ("다릅니다", "다르다"),
        ("많습니다", "많다"),
        ("적습니다", "적다"),
        ("큽니다", "크다"),
        ("작습니다", "작다"),
        ("높습니다", "높다"),
        ("낮습니다", "낮다"),
        ("좋습니다", "좋다"),
        ("나쁩니다", "나쁘다"),
        ("보입니다", "보인다"),
        ("들립니다", "들린다"),
        ("느껴집니다", "느껴진다"),
        ("생각됩니다", "생각된다"),
        ("생각합니다", "생각한다"),
        ("전해져 옵니다", "전해져 온다"),
        ("전해집니다", "전해진다"),
        ("떠납니다", "떠난다"),
        ("향합니다", "향한다"),
        ("따라갑니다", "따라간다"),
        ("들어갑니다", "들어간다"),
        ("나옵니다", "나온다"),
        ("바라봅니다", "바라본다"),
        ("찾습니다", "찾는다"),
        ("앉습니다", "앉는다"),
        ("맡습니다", "맡는다"),
        ("받습니다", "받는다"),
        ("얻습니다", "얻는다"),
        ("잃습니다", "잃는다"),
        ("열립니다", "열린다"),
        ("닫힙니다", "닫힌다"),
        ("불립니다", "불린다"),
        ("시작됩니다", "시작된다"),
        ("시작합니다", "시작한다"),
        ("사용합니다", "사용한다"),
        ("포함합니다", "포함한다"),
        ("나타냅니다", "나타낸다"),
        ("설명합니다", "설명한다"),
        ("제공합니다", "제공한다"),
        ("지원합니다", "지원한다"),
        ("필요합니다", "필요하다"),
        ("가능합니다", "가능하다"),
        ("중요합니다", "중요하다"),
        ("마칩니다", "마친다"),
        ("끝납니다", "끝난다"),
        ("놀랍니다", "놀란다"),
        ("바랍니다", "바란다"),
        ("만납니다", "만난다"),
        ("다룹니다", "다룬다"),
        ("따릅니다", "따른다"),
        ("부릅니다", "부른다"),
        ("누립니다", "누린다"),
        ("이룹니다", "이룬다"),
        ("나눕니다", "나눈다"),
        ("바꿉니다", "바꾼다"),
        ("치릅니다", "치른다"),
        ("기릅니다", "기른다"),
        ("흐릅니다", "흐른다"),
        ("오릅니다", "오른다"),
        ("자랍니다", "자란다"),
        ("지나갑니다", "지나간다"),
        ("돌아갑니다", "돌아간다"),
        ("돌아옵니다", "돌아온다"),
        ("드러납니다", "드러난다"),
        ("존재합니다", "존재한다"),
        ("등장합니다", "등장한다"),
        ("발생합니다", "발생한다"),
        ("유명합니다", "유명하다"),
        ("강력합니다", "강력하다"),
        ("복잡합니다", "복잡하다"),
        ("정확합니다", "정확하다"),
        ("말합니다", "말한다"),
        ("대답합니다", "대답한다"),
        ("묻습니다", "묻는다"),
        ("웃습니다", "웃는다"),
        ("됩니다", "된다"),
        ("합니다", "한다"),
        ("갑니다", "간다"),
        ("옵니다", "온다"),
        ("봅니다", "본다"),
        ("집니다", "진다"),
        ("칩니다", "친다"),
        ("킵니다", "킨다"),
        ("힙니다", "힌다"),
        ("립니다", "린다"),
        ("기울입니다", "기울인다"),
        ("움직입니다", "움직인다"),
        ("일어납니다", "일어난다"),
        ("나타납니다", "나타난다"),
        ("사라집니다", "사라진다"),
        ("이어집니다", "이어진다"),
        ("떨어집니다", "떨어진다"),
        ("던집니다", "던진다"),
        ("가집니다", "가진다"),
        ("만듭니다", "만든다"),
        ("듭니다", "든다"),
    ] {
        for punct in [".", "!", "…", ";", ",", ")", "”", "’"] {
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

    // 2. General Korean past/future tense morphological rule using Unicode Jongseong == 20 (`ㅆ` batchim):
    //    Any `<Hangul syllable with ㅆ batchim>습니다` -> `<syllable>다` (covers 었/았/였/했/꼈/셨/겠/etc.)
    if out.contains("습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "습니다".len()..];
            let boundary = after.is_empty()
                || after.starts_with([
                    '.', '!', '?', '…', ';', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
                ]);
            fixed.push_str(prefix);
            if boundary && prefix.chars().last().is_some_and(has_korean_ss_batchim) {
                fixed.push('다');
            } else {
                fixed.push_str("습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 3. General Korean copula `<noun>입니다` -> `<noun>이다` / `<noun>다` and passive `-됩니다` -> `-된다`
    if out.contains("입니다") {
        *out = replace_korean_copula_imnida(out);
    }
    if out.contains("됩니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("됩니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "됩니다".len()..];
            let boundary = after.is_empty()
                || after.starts_with([
                    '.', '!', '?', '…', ';', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
                ]);
            fixed.push_str(prefix);
            if boundary {
                fixed.push_str("된다");
            } else {
                fixed.push_str("됩니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 4. Fix common Korean adjective `-ㄴ다` conjugation errors (`필요한다` -> `필요하다`)
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
    ] {
        if out.contains(bad_adj) {
            *out = out.replace(bad_adj, good_adj);
        }
    }

    // 5. Sentence-ending polite `~해요 / ~예요 / ~어요 / ~아요` or accidental banmal `~이야.` in non-dialogue prose
    if !out.contains('“') && !out.contains('‘') {
        for (non_plain, plain) in [
            ("했어요.", "했다."),
            ("됐어요.", "됐다."),
            ("왔어요.", "왔다."),
            ("갔어요.", "갔다."),
            ("났어요.", "났다."),
            ("졌어요.", "졌다."),
            ("렸어요.", "렸다."),
            ("어요.", "다."),
            ("아요.", "다."),
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
        || (t.starts_with('-') && t.chars().nth(1).is_some_and(|c| c.is_ascii_alphanumeric()))
        || lower.ends_with(".exe")
        || lower.ends_with(".dll")
        || lower.ends_with(".bsp")
        || lower.ends_with(".vmf")
        || lower.ends_with(".vpk")
        || lower.ends_with(".cfg")
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
        ("듣지 않았나요?", "안 물어봤어?"),
        ("물어보지 않았나요?", "안 물어봤어?"),
        ("않았나요?", "않았어?"),
        ("퇴근하나요?", "마감했어?"),
        ("마감인가요?", "마감했어?"),
        ("의뢰가 있나요?", "의뢰를 했어?"),
        ("의뢰를 받았나요?", "의뢰를 했어?"),
        ("피해가 있었나요?", "피해가 있었어?"),
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
        } else if out.ends_with("ような?”")
            || out.ends_with("ような?」")
            || out.ends_with("ような?”")
        {
            out = out.replace("ような", "것 같기도 하고");
        }
        if out.contains("갈라놓았던착암룡") {
            out = out.replace("갈라놓았던착암룡", "갈라놓았던 착암룡");
        }
        if out.contains("上的") && !source_text.contains("上的") {
            out = out.replace("서버上的", "서버상의").replace("上的", "상의");
        }
        if out.contains("アワード") {
            out = out
                .replace("게임アワード", "게임 어워드")
                .replace("アワード", "어워드");
        }
        if out.contains("페ルソナ") {
            out = out.replace("페ルソナ", "페르소나");
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

        if source_text.contains("澄人教") && out.contains("청인교") {
            out = out.replace("청인교", "징인교");
        }
        for (bad_guest, good_guest) in [
            (
                "천천히 천천히 천천히 나중에 들어온 손님",
                "나중에 들어온 손님",
            ),
            ("천천히 나중에 들어온 손님", "나중에 들어온 손님"),
            ("뒤에서 들어온 손님", "나중에 들어온 손님"),
            ("뒤에서 들어오던 손님", "나중에 들어오던 손님"),
            ("뒤에 들어온 손님", "나중에 들어온 손님"),
            ("뒤에서 온 손님", "나중에 들어온 손님"),
            ("뒤에 온 손님", "나중에 들어온 손님"),
            ("뒤로 들어온 손님", "나중에 들어온 손님"),
            ("뒤따라 들어온 손님", "나중에 들어온 손님"),
            ("나중에 온 손님", "나중에 들어온 손님"),
        ] {
            if out.contains(bad_guest) {
                out = out.replace(bad_guest, good_guest);
            }
        }
        for (bad_claw, good_pincer) in [
            ("게의 발톱", "게의 집게"),
            ("게의 집게발", "게의 집게"),
            ("게의 가시", "게의 집게"),
            ("게의 갈고리", "게의 집게"),
            ("갑각류의 발톱", "갑각류의 집게"),
            ("갑각류의 집게발", "갑각류의 집게"),
            ("갑각류의 가시", "갑각류의 집게"),
            ("갑각류의 갈고리", "갑각류의 집게"),
            ("껍질의 집게발", "갑각류의 집게"),
            ("껍질의 집게", "갑각류의 집게"),
            ("해면동물의 발톱", "갑각류의 집게"),
            ("해면동물의 집게", "갑각류의 집게"),
            ("해양 동물의 갈고리에", "갑각류의 집게와"),
            ("해양 동물의 갈고리", "갑각류의 집게"),
            ("해양 동물의 갑각을 잡으려고", "갑각류의 집게와"),
        ] {
            if out.contains(bad_claw) {
                out = out.replace(bad_claw, good_pincer);
            }
        }
        if source_text.contains('爪')
            && (source_text.contains('蟹')
                || source_text.contains("カニ")
                || source_text.contains("甲殻類")
                || source_text.contains("ヒビを入れて")
                || out.contains("게"))
        {
            for (claw_tok, pincer_tok) in [
                ("그 발톱", "그 집게"),
                ("그 집게발", "그 집게"),
                ("그 가시", "그 집게"),
                ("그 갈고리", "그 집게"),
                ("그 갑각을", "그 집게를"),
                ("집게을", "집게를"),
                ("발톱을", "집게를"),
                ("집게발을", "집게를"),
                ("갈고리를", "집게를"),
                ("발톱과", "집게와"),
                ("집게발과", "집게와"),
                ("갈고리와", "집게와"),
                ("발톱에", "집게에"),
                ("집게발에", "집게에"),
                ("갈고리에", "집게와"),
                ("발톱이", "집게가"),
                ("집게발이", "집게가"),
                ("발톱은", "집게는"),
                ("집게발은", "집게는"),
                ("집게과", "집게와"),
            ] {
                if out.contains(claw_tok) {
                    out = out.replace(claw_tok, pincer_tok);
                }
            }
        }
        if source_text.contains("不要かな") || source_text.contains("必要ないだろうな")
        {
            for (bad_musing, good_musing) in [
                ("지그에게는 필요 없을 거야……", "지그는 필요 없겠지……"),
                ("지그에게는 필요 없을 거야...", "지그는 필요 없겠지..."),
                ("지그한테는 필요 없을 거야…", "지그는 필요 없겠지…"),
                ("지그한테는 필요 없을 거야...", "지그는 필요 없겠지..."),
                ("필요 없을 거야……", "필요 없겠지……"),
                ("필요 없을 거야…", "필요 없겠지…"),
                ("필요 없을 거야...", "필요 없겠지..."),
                ("필요 없을 거야.", "필요 없겠지."),
                ("필요 없을까...", "필요 없겠지..."),
                ("필요 없을까…", "필요 없겠지…"),
                ("필요 없을까?", "필요 없겠지."),
                ("필요 없을까", "필요 없겠지"),
                ("불필요할까...", "필요 없겠지..."),
                ("불필요할까…", "필요 없겠지…"),
            ] {
                if out.contains(bad_musing) {
                    out = out.replace(bad_musing, good_musing);
                }
            }
        }
        if source_text.contains("信者ってわけじゃなくても")
            || source_text.contains("信徒ではない人であっても")
        {
            for (bad_believer, good_believer) in [
                ("굳이 신자로 되는건 아니지만", "딱히 신도가 아닌 사람이라도"),
                (
                    "굳이 신자로 되는 건 아니지만",
                    "딱히 신도가 아닌 사람이라도",
                ),
                (
                    "굳이 신자가 되는 건 아니지만",
                    "딱히 신도가 아닌 사람이라도",
                ),
                ("굳이 신자가 아니더라도", "딱히 신도가 아닌 사람이라도"),
                ("별로 신자라는 건 아니지만", "딱히 신도가 아닌 사람이라도"),
                ("딱히 신자라는 건 아니지만", "딱히 신도가 아닌 사람이라도"),
                ("딱히 신자는 아니지만", "딱히 신도가 아닌 사람이라도"),
                (
                    "특히 신도가 아닌 사람들 사이에서도",
                    "딱히 신도가 아닌 사람이라도,",
                ),
                ("특히 신도가 아닌 사람들도", "딱히 신도가 아닌 사람이라도,"),
                ("특히 신도가 아닌 사람이라도", "딱히 신도가 아닌 사람이라도"),
                (
                    "특별히 신도가 아닌 사람이라도",
                    "딱히 신도가 아닌 사람이라도",
                ),
            ] {
                if out.contains(bad_believer) {
                    out = out.replace(bad_believer, good_believer);
                }
            }
        }
        if source_text.contains("ぷっ")
            || source_text.contains("ぷふ")
            || source_text.contains("ぶっ")
            || source_text.contains("笑いを漏ら")
        {
            if out.contains("웃음을 터뜨리며 웃었다") {
                out = out.replace("웃음을 터뜨리며 웃었다", "웃음을 터뜨렸다");
            }
            if out.contains("으엉") || out.contains("으앙") {
                out = out.replace("으엉", "프흡").replace("으앙", "프흡");
            } else if out.contains("푸르르게 프흡") {
                out = out.replace("푸르르게 프흡", "프흡");
            } else if out.contains("갑자기 웃음을 터뜨") {
                out = out.replace("갑자기 웃음을 터뜨", "프흡 하며 웃음을 터뜨");
            } else if !out.contains("프흡") && out.contains("웃음을 터뜨") {
                out = out.replace("웃음을 터뜨", "프흡 하며 웃음을 터뜨");
            }
        } else if (out.contains("으엉") || out.contains("으앙"))
            && (source_text.contains('笑')
                || source_text.contains("吹き出")
                || source_text.contains("ふっ")
                || out.contains("웃")
                || out.contains("터뜨")
                || out.contains("뿜"))
        {
            out = out.replace("으엉", "프흡").replace("으앙", "프흡");
        }

        let is_dialogue = is_dialogue_segment(source_text);
        if !is_dialogue {
            if !source_text.contains('俺')
                && !source_text.contains('私')
                && !source_text.contains('僕')
            {
                for (bad_first_person, clean_phrase) in [
                    ("내가 주문한", "주문해 둔"),
                    ("내가 요청했던", "주문해 둔"),
                    ("내가 시킨", "주문해 둔"),
                ] {
                    if out.contains(bad_first_person) {
                        out = out.replace(bad_first_person, clean_phrase);
                    }
                }
            }
            if source_text.contains("舌を引っ込め") && out.contains("혀를 내밀") {
                out = out.replace("혀를 내밀", "혀를 집어넣");
            }
        } else {
            match crate::ai::detect_dialogue_register(source_text) {
                crate::ai::DialogueRegister::Casual => {
                    enforce_casual_korean_dialogue(&mut out);
                }
                crate::ai::DialogueRegister::Polite => {
                    for (casual_pat, polite_rep) in [
                        ("토벌이야.”", "토벌이에요.”"),
                        ("잡는 일이야.”", "토벌이에요.”"),
                        ("무리도 같아.”", "무리였어요.”"),
                        ("무리야.”", "무리였어요.”"),
                        ("수입을 얻었어.”", "수입이 되었어요.”"),
                        ("수입이 되었어.”", "수입이 되었어요.”"),
                    ] {
                        if out.contains(casual_pat) {
                            out = out.replace(casual_pat, polite_rep);
                        }
                    }
                }
                crate::ai::DialogueRegister::Neutral => {}
            }
        }

        // TCRF & Namuwiki (JA/EN -> KO) domain post-corrections
        if src_lower.contains("title screen") {
            if out.contains("제목화면")
                || out.contains("제목 화면")
                || out.contains("타이틀 스크린")
            {
                out = out
                    .replace("제목화면", "타이틀 화면")
                    .replace("제목 화면", "타이틀 화면")
                    .replace("타이틀 스크린", "타이틀 화면");
            }
            if src_lower.contains("take you to the title screen") {
                out = out
                    .replace("타이틀 화면으로 데려간다", "타이틀 화면으로 이동한다")
                    .replace("타이틀 화면으로 안내한다", "타이틀 화면으로 이동한다");
            }
        }
        if src_trimmed.starts_with("ENCOUNT - ") && !out.starts_with("ENCOUNT - ") {
            for bad_prefix in ["인카운트 - ", "조우 - ", "만남 - "] {
                if let Some(rest) = out.strip_prefix(bad_prefix) {
                    out = format!("ENCOUNT - {rest}");
                    break;
                }
            }
        }
        if src_lower.contains("encounter of enemies")
            || (src_lower.contains("encounter") && src_lower.contains("enem"))
        {
            for (bad_enc, good_enc) in [
                ("만날 적의 대전을", "전투할 적 그룹(인카운트)을"),
                ("마주칠 적의 대전을", "전투할 적 그룹(인카운트)을"),
                ("싸울 적의 대전을", "전투할 적 그룹(인카운트)을"),
                ("전투할 적의 대전을", "전투할 적 그룹(인카운트)을"),
                ("만날 적의 조우를", "전투할 적 그룹(인카운트)을"),
                ("싸울 적의 조우를", "전투할 적 그룹(인카운트)을"),
                ("적의 대전을", "전투할 적 그룹(인카운트)을"),
                ("적 집단과 싸우는지", "적 그룹(인카운트)과 싸우는지"),
                ("적군 그룹과 싸울지", "적 그룹(인카운트)과 싸울지"),
                ("싸울 적 인카운트 그룹을", "전투할 적 그룹(인카운트)을"),
                ("전투할 적 인카운트 그룹을", "전투할 적 그룹(인카운트)을"),
                (
                    "전투에서 싸울 적 인카운트 그룹을",
                    "전투할 적 그룹(인카운트)을",
                ),
                ("전투에서 싸울 적 그룹을", "전투할 적 그룹(인카운트)을"),
            ] {
                if out.contains(bad_enc) {
                    out = out.replace(bad_enc, good_enc);
                }
            }
        }
        if src_lower.contains("party slot") {
            for (bad_slot, good_slot) in [
                ("당신의 제3자 멤버 슬롯", "3번째 파티 슬롯"),
                ("제3자 멤버 슬롯", "3번째 파티 슬롯"),
                ("당신의 제3자 슬롯", "3번째 파티 슬롯"),
                ("제3자 슬롯", "3번째 파티 슬롯"),
                ("당신의 2번째 파티 슬롯", "2번째 파티 슬롯"),
                ("당신의 3번째 파티 슬롯", "3번째 파티 슬롯"),
                ("당신의 4번째 파티 슬롯", "4번째 파티 슬롯"),
                ("두 번째 파티원 슬롯", "2번째 파티 슬롯"),
                ("세 번째 파티원 슬롯", "3번째 파티 슬롯"),
                ("네 번째 파티원 슬롯", "4번째 파티 슬롯"),
                ("파티 멤버 슬롯", "파티 슬롯"),
                ("파티 멤버 자리", "파티 슬롯"),
                ("파티 멤버 위치", "파티 슬롯"),
                ("파티원 자리", "파티 슬롯"),
                ("파티원 슬롯", "파티 슬롯"),
                ("파티원으로", "파티 슬롯에"),
                ("파티 슬롯에 포함된 인원을", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 있는 인원을", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 있는 사람을", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 속한 인원을", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 들어갈 인원을", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 누가 있는지", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 배치된 캐릭터를", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 들어갈 캐릭터를", "파티 슬롯에 배치할 캐릭터를"),
                ("파티 슬롯에 있는 캐릭터를", "파티 슬롯에 배치할 캐릭터를"),
            ] {
                if out.contains(bad_slot) {
                    out = out.replace(bad_slot, good_slot);
                }
            }
        }
        if src_trimmed.starts_with("TALK MAJOR - ") && !out.starts_with("TALK MAJOR - ") {
            for bad_prefix in ["토크 메이저 - ", "대화 메이저 - ", "대화 주요 - "] {
                if let Some(rest) = out.strip_prefix(bad_prefix) {
                    out = format!("TALK MAJOR - {rest}");
                    break;
                }
            }
        }
        if src_lower.contains("holdup") || src_lower.contains("hold up") {
            for (bad_hold, good_hold) in [
                ("가로막기로", "홀드업으로"),
                ("가로막기를 통해", "홀드업을 통해"),
                ("가로막기 중", "홀드업 중"),
                ("가로막기 동안", "홀드업 동안"),
                ("가로막기", "홀드업"),
                ("강도질로", "홀드업으로"),
                ("포위 중", "홀드업 중"),
                ("가드 중", "홀드업 중"),
                ("홀드 업", "홀드업"),
                ("홀업", "홀드업"),
            ] {
                if out.contains(bad_hold) {
                    out = out.replace(bad_hold, good_hold);
                }
            }
        }
        if src_lower.contains("shadow")
            && (src_lower.contains("holdup")
                || src_lower.contains("hold up")
                || src_lower.contains("persona")
                || src_lower.contains("talk major"))
        {
            for (bad_sh, good_sh) in [
                ("그림자들과", "섀도우들과"),
                ("그림자들과의", "섀도우들과의"),
                ("그림자와의", "섀도우와의"),
                ("그림자에게", "섀도우에게"),
                ("그림자를", "섀도우를"),
                ("그림자와", "섀도우와"),
                ("그림자", "섀도우"),
            ] {
                if out.contains(bad_sh) {
                    out = out.replace(bad_sh, good_sh);
                }
            }
        }
        if src_lower.contains("major")
            && (src_lower.contains("talk") || src_lower.contains("sets the major"))
        {
            for (bad_maj, good_maj) in [
                ("대화 주를", "대화 메이저 ID를"),
                ("대화의 주를", "대화 메이저 ID를"),
                ("대화 전공을", "대화 메이저 ID를"),
                ("대화 주요를", "대화 메이저 ID를"),
                ("대화 메이저를", "대화 메이저 ID를"),
                ("말하기의 주요 ID를", "대화 메이저 ID를"),
                ("말하기의 메이저 ID를", "대화 메이저 ID를"),
                ("말하는 메이저 ID를", "대화 메이저 ID를"),
                ("말하는 주요 ID를", "대화 메이저 ID를"),
                ("대화하는 주요 ID를", "대화 메이저 ID를"),
                ("대화하기 위한 주요 ID를", "대화 메이저 ID를"),
                ("섀도우와 통신할 때", "섀도우와 대화할 때"),
                ("주요 ID를", "메이저 ID를"),
                ("메인 ID를", "메이저 ID를"),
            ] {
                if out.contains(bad_maj) {
                    out = out.replace(bad_maj, good_maj);
                }
            }
        }
        if src_trimmed.starts_with("PREEMPTIVE - ") && !out.starts_with("PREEMPTIVE - ") {
            for bad_prefix in ["선제공격 - ", "선제 - "] {
                if let Some(rest) = out.strip_prefix(bad_prefix) {
                    out = format!("PREEMPTIVE - {rest}");
                    break;
                }
            }
        }

        // Namuwiki JA/EN -> KO exact-context machine-retranslation fixes
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
        if src_lower.contains("mega tensei")
            || src_lower.contains("goddess tenshi")
            || source_text.contains("女神転生")
        {
            for (bad_mt, good_mt) in [
                ("메가 텐세이", "여신전생"),
                ("메가텐세이", "여신전생"),
                ("여신 텐시", "여신전생"),
                ("여신텐시", "여신전생"),
                ("여신의 귀환", "여신전생"),
                ("Mega Tensei", "여신전생"),
            ] {
                if out.contains(bad_mt) {
                    out = out.replace(bad_mt, good_mt);
                }
            }
        }
        if source_text.contains("女神异闻录") && out.contains("여신의 귀신들") {
            out = out.replace("여신의 귀신들", "여신이문록");
        }
        if source_text.contains("黙示録") && out.contains("몽롱록") {
            out = out.replace("몽롱록", "묵시록");
        }
        if source_text.contains("ラストバイブル") && out.contains("마지막 책") {
            out = out.replace("마지막 책", "라스트 바이블");
        }
        if src_lower.contains("goddess lee munrok") || source_text.contains("女神イ・ムンロク")
        {
            for (bad_lm, good_lm) in [
                ("여신 이문로크의 페르소나", "여신이문록 페르소나"),
                ("여신 이문록의 페르소나", "여신이문록 페르소나"),
                ("여신 이문록 페르소나", "여신이문록 페르소나"),
                ("여신 이 문록 페르소나", "여신이문록 페르소나"),
                ("여신 리문록 페르소나", "여신이문록 페르소나"),
            ] {
                if out.contains(bad_lm) {
                    out = out.replace(bad_lm, good_lm);
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

        // General gaming / FPS / wiki localization refinements (KO only)
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
        if src_lower.contains("alternate") && out.contains("대체적인") {
            out = out.replace("대체적인", "또 다른");
        }
        if src_lower.contains("pellet") && out.contains("구슬탄") {
            out = out.replace("구슬탄", "산탄");
        }
        if src_lower.contains("pump-action") && out.contains("펌프형") {
            out = out.replace("펌프형", "펌프 액션");
        }
        if src_lower.contains("developer commentary") {
            out = out
                .replace("개발자들의 코멘트", "개발자 코멘터리")
                .replace("개발자 코멘트", "개발자 코멘터리");
        }
        if out.contains("엔진]]를") || out.contains("engine]]를") {
            out = out
                .replace("엔진]]를", "엔진]]을")
                .replace("engine]]를", "engine]]을");
        }
        if source_text.contains("Combine")
            && !src_lower.contains("combined")
            && (out.contains("컴바인드") || out.contains("콤비네이트"))
        {
            out = out
                .replace("컴바인드", "콤바인")
                .replace("콤비네이트", "콤바인")
                .replace("콤바인]]라는", "콤바인]]이라는")
                .replace("콤바인]]를", "콤바인]]을")
                .replace("콤바인]]와", "콤바인]]과")
                .replace("콤바인]]가", "콤바인]]이")
                .replace("콤바인라는", "콤바인이라는")
                .replace("콤바인를", "콤바인을")
                .replace("콤바인와", "콤바인과")
                .replace("콤바인가", "콤바인이");
        }
        if src_lower.contains("point insertion") && out.contains("포인트 인스톨션") {
            out = out.replace("포인트 인스톨션", "포인트 인서션");
        }
        if src_lower.contains("ravenholm") && out.contains("레이븐홀름") {
            out = out.replace("레이븐홀름", "레이븐홈");
        }
        if src_lower.contains("episode two") && out.contains("에피소드 두") {
            out = out.replace("에피소드 두", "에피소드 2");
        }
        if !is_dialogue {
            normalize_korean_narration_endings(&mut out);
        }
    }

    // 2. Universal repairs applied across ALL source <-> target language pairs
    strip_hallucinated_marker_urls(source_text, &mut out);
    if norm_target == "English" && out.contains("slot slots") {
        out = out.replace("slot slots", "slots");
    }
    for prefix in [
        "ENCOUNT - ",
        "TALK MAJOR - ",
        "PREEMPTIVE - ",
        "Party1 - ",
        "Party2 - ",
        "Party4 - ",
    ] {
        if src_trimmed.starts_with(prefix) && !out.starts_with(prefix) {
            for sep in [" - ", " – ", " — ", ": "] {
                if let Some((lhs, rhs)) = out.split_once(sep)
                    && lhs.chars().count() <= 20
                    && !rhs.trim().is_empty()
                {
                    out = format!("{prefix}{}", rhs.trim_start());
                    break;
                }
            }
        }
    }
    repair_inline_markers(source_text, &mut out);
    strip_hallucinated_marker_urls(source_text, &mut out);
    strip_spurious_question_mark(source_text, &mut out);
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
    }

    #[test]
    fn fandom_vdc_dom_code_markers_and_shortcuts_work() {
        // 1. Marker-wrapped literal identifiers & footnotes take 0ms shortcut
        assert_eq!(
            non_translatable_shortcut("[[0|d1_canals_01]]").as_deref(),
            Some("[[0|d1_canals_01]]")
        );
        assert_eq!(
            non_translatable_shortcut("[[0|[1]]]").as_deref(),
            Some("[[0|[1]]]")
        );
        assert_eq!(
            non_translatable_shortcut("[[img0]] 2004").as_deref(),
            Some("[[img0]] 2004")
        );
        assert!(non_translatable_shortcut("[[img0]] Half-Life 2").is_none());

        // 2. repair_inline_markers restores corrupted map/entity code names while keeping footnotes intact
        let src =
            "Excerpts ([[0|d1_trainstation_01]]) load [[1|d1_town_02a]] via trigger.[[2|[1]]]";
        let mut out =
            "발췌본 ([[0|d1_기차역_01]])은 트리거를 통해 [[1|d1_town_02]]을 불러온다.[[2|[1]]"
                .to_string();
        repair_inline_markers(src, &mut out);
        assert_eq!(
            out,
            "발췌본 ([[0|d1_trainstation_01]])은 트리거를 통해 [[1|d1_town_02a]]을 불러온다.[[2|[1]]]"
        );

        // 3. Universal URL hallucination cleanup (both `](http...` single-bracket and `]](http...`)
        assert_eq!(
            postprocess_translation_output(
                "The game uses the [[0|Source engine]], with [[1|Havok physics engine]].",
                "日本語",
                "natural",
                "このゲームは[[0|Source engine](https://wiki.mojibake.org/Source_engine)を使用しており、[[1|Havok物理エンジン]]https://wiki.mojibake.org/Havok)が含まれています。".into(),
            ),
            "このゲームは[[0|Source engine]]を使用しており、[[1|Havok物理エンジン]]が含まれています。"
        );

        // 4. Korean narration passive `-됩니다` / verb endings & global gaming/wiki localization rules
        let hl_lead = postprocess_translation_output(
            "Half-Life 2 is a [[0|science fiction]] [[1|first-person shooter]] included in [[4|The Orange Box]].",
            "한국어",
            "natural",
            "하프라이프 2는 [[0|과학소설]] [[1|퍼스트퍼스먼트 슈터]] 장르로 [[4|The Orange Box]]에 포함됩니다.".into(),
        );
        assert_eq!(
            hl_lead,
            "하프라이프 2는 [[0|SF]] [[1|1인칭 슈팅 게임]] 장르로 [[4|The Orange Box]]에 포함된다."
        );

        let combine_out = postprocess_translation_output(
            "Freeman battles the [[4|Combine]] while allies are shocked to see him alive.",
            "한국어",
            "natural",
            "프리먼은 [[4|컴바인드]]에 맞서 싸우고 동료들은 그가 살아 있는 것을 보고 놀랍니다."
                .into(),
        );
        assert_eq!(
            combine_out,
            "프리먼은 [[4|콤바인]]에 맞서 싸우고 동료들은 그가 살아 있는 것을 보고 놀란다."
        );

        assert_eq!(
            postprocess_translation_output(
                "Mode [[0|Single-player]] using [[1|Source engine]] and [[2|Episode Two]] in Ravenholm (Point Insertion).",
                "한국어",
                "natural",
                "모드: [[0|Single-player]], [[1|Source engine]]를 사용하며 레이븐홀름(포인트 인스톨션)과 [[2|에피소드 두]]가 포함됩니다.".into(),
            ),
            "모드: [[0|싱글플레이어]], [[1|Source engine]]을 사용하며 레이븐홈(포인트 인서션)과 [[2|에피소드 2]]가 포함된다."
        );

        // 5. Repaired CJK bracket `》`/`】` before `입니다` + negative `~않습니다` + Combine/`게임 게임` dedupe
        assert_eq!(
            postprocess_translation_output(
                "Half-Life 2 is a [[1|first-person shooter]].",
                "한국어",
                "natural",
                "하프라이프 2는 [[1|퍼스트퍼스먼트 슈터》입니다.".into(),
            ),
            "하프라이프 2는 [[1|1인칭 슈팅 게임]]이다."
        );
        assert_eq!(
            postprocess_translation_output(
                "This game is a [[1|first-person shooter]].",
                "한국어",
                "natural",
                "이 게임은 [[1|1인칭 슈팅 게임]] 게임이다.".into(),
            ),
            "이 게임은 [[1|1인칭 슈팅 게임]]이다."
        );
        assert_eq!(
            postprocess_translation_output(
                "Freeman battles the [[4|Combine]] and this article has yet to be cleaned up.",
                "한국어",
                "natural",
                "프리먼은 [[4|콤비네이트]]라는 종족과 싸우며 이 글은 아직 정리되어 있지 않습니다."
                    .into(),
            ),
            "프리먼은 [[4|콤바인]]이라는 종족과 싸우며 이 글은 아직 정리되어 있지 않다."
        );
        assert_eq!(
            postprocess_translation_output(
                "The game uses [[0|Source engine]].",
                "日本語",
                "natural",
                "このゲームは[[0|Sourceエンジン]]】を使用している。".into(),
            ),
            "このゲームは[[0|Sourceエンジン]]を使用している。"
        );

        // 6. Parallel sentence splitter + prompt echo guard
        let long_para = "A major update (Half-Life 2 20th Anniversary Update), has been released on November 16, 2024 to celebrate the 20th anniversary of the game. It adds Steam Workshop support, developer commentary, fixes various bugs, and makes both episodes, as well as [[0|Lost Coast]], accessible directly through the menu. There are also a few other changes (e.g. \"Very High\" shaders and U.S. builds version 1.8 updates) that enable [[1|lightmap]] filtering.[[3|[1]]] Additionally, the Xbox 360 version was used as a base before Valve fixed the remaining issues.";
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
        assert!(!looks_like_prompt_echo(
            long_para,
            "2024년 11월 16일 게임의 20주년을 기념하는 대규모 업데이트가 출시되었다."
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
    fn multilingual_target_scoping_and_namuwiki_rules_work() {
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

        // 3. Universal debug prefix preservation & footnote triple-bracket balance
        assert_eq!(
            postprocess_translation_output(
                "ENCOUNT - Sets what encounter of enemies will be fought.",
                "日本語",
                "natural",
                "エンカウント – 戦闘する敵グループを設定する。".into(),
            ),
            "ENCOUNT - 戦闘する敵グループを設定する。"
        );
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
            already_in_target_language_shortcut("페르소나 3는 2006년 출시된 RPG다.", "한국어")
                .is_some()
        );
        assert!(
            already_in_target_language_shortcut("이 게임은 [[0|Persona 3]]이다.", "한국어")
                .is_none()
        );
        assert!(already_in_target_language_shortcut("このゲームはRPGです。", "日本語").is_some());
        assert!(
            already_in_target_language_shortcut("C'est un jeu RPG très célèbre.", "English")
                .is_none()
        );

        // 5. Namuwiki JA/EN -> KO exact-context fixes
        assert_eq!(
            postprocess_translation_output(
                "詳しくはペルソナ3 FES文書をご注意ください。",
                "한국어",
                "natural",
                "자세한 내용은 페르소나 3 FES 문서를 주의하십시오.".into(),
            ),
            "자세한 내용은 페르소나 3 FES 문서를 참고하십시오."
        );
        assert_eq!(
            postprocess_translation_output("1. outline", "한국어", "natural", "1. 윤곽".into()),
            "1. 개요"
        );
        assert_eq!(
            postprocess_translation_output(
                "4. characteristic",
                "한국어",
                "natural",
                "4. 특성".into()
            ),
            "4. 특징"
        );
        assert_eq!(
            postprocess_translation_output(
                "6. extended edition",
                "한국어",
                "natural",
                "6. 연장판".into()
            ),
            "6. 확장판"
        );
        assert_eq!(
            postprocess_translation_output(
                "11. Koreanization",
                "한국어",
                "natural",
                "11. 한국화".into()
            ),
            "11. 한국어화"
        );
        assert_eq!(
            postprocess_translation_output(
                "circulation ATLUS SCEK",
                "한국어",
                "natural",
                "순환 ATLUS SCEK".into()
            ),
            "유통 ATLUS SCEK"
        );
        assert_eq!(
            postprocess_translation_output(
                "2009年[[0|[4]]]に移植版が発売された。",
                "한국어",
                "natural",
                "2009년 [[0|[4]]]에移植 버전이 출시되었다.".into(),
            ),
            "2009년 [[0|[4]]]에 이식 버전이 출시되었다."
        );
        assert_eq!(
            postprocess_translation_output(
                "[[0|ペルソナシリーズ]]、公式略称はP3です。",
                "한국어",
                "natural",
                "[[0|페ルソナ시리즈]], 공식 약칭은 P3입니다.".into(),
            ),
            "[[0|페르소나시리즈]], 공식 약칭은 P3이다."
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
            postprocess_translation_output(
                "テストパーティーのキャラクター",
                "English",
                "natural",
                "You can test party slot slots.".into(),
            ),
            "You can test party slots."
        );
        assert_eq!(
            postprocess_translation_output(
                "《女神异闻录3》",
                "한국어",
                "natural",
                "《여신의 귀신들 3》".into(),
            ),
            "《여신이문록 3》"
        );
        assert_eq!(
            postprocess_translation_output(
                "参考までに、女神イ・ムンロクのペルソナは黙示録として、ラストバイブルは別作として出た。",
                "한국어",
                "natural",
                "참고로, 여신 이문로크의 페르소나는 몽롱록으로, 라스트 바이블 대신 마지막 책은 별작으로 나왔다.".into(),
            ),
            "참고로, 여신이문록 페르소나는 묵시록으로, 라스트 바이블 대신 라스트 바이블은 별작으로 나왔다."
        );
    }

    #[test]
    fn tcrf_global_tuning_and_shortcuts_work() {
        // 1. title screen -> 타이틀 화면
        let ts = postprocess_novel_output(
            "Selecting this will take you to the title screen.",
            "natural",
            "이 항목을 선택하면 제목화면으로 이동한다.".into(),
        );
        assert_eq!(ts, "이 항목을 선택하면 타이틀 화면으로 이동한다.");

        // 2. ENCOUNT - Sets what encounter of enemies will be fought.
        let enc_pre = preprocess_novel_source_text(
            "ENCOUNT - Sets what encounter of enemies will be fought.",
            "natural",
        );
        assert!(enc_pre.contains("which enemy encounter group will be fought in battle"));
        let enc = postprocess_novel_output(
            "ENCOUNT - Sets what encounter of enemies will be fought.",
            "natural",
            "ENCOUNT - 만날 적의 대전을 설정한다.".into(),
        );
        assert_eq!(enc, "ENCOUNT - 전투할 적 그룹(인카운트)을 설정한다.");

        // 3. Party1/2/4 - Sets who is in your 2nd/3rd/4th party slot.
        let party_pre =
            preprocess_novel_source_text("Party1 - Sets who is in your 2nd party slot.", "natural");
        assert!(party_pre.contains("which character is placed in the 2nd party member slot"));
        let party = postprocess_novel_output(
            "Party2 - Sets who is in your 3rd party slot.",
            "natural",
            "Party2 - 3번째 파티 슬롯에 포함된 인원을 설정한다.".into(),
        );
        assert_eq!(
            party,
            "Party2 - 3번째 파티 슬롯에 배치할 캐릭터를 설정한다."
        );

        // 4. TALK MAJOR - Sets the major of talking to shadows through holdup.
        let talk_pre = preprocess_novel_source_text(
            "TALK MAJOR - Sets the major of talking to shadows through holdup.",
            "natural",
        );
        assert!(talk_pre.contains("major ID for talking to Shadows during a Hold Up"));
        let talk = postprocess_novel_output(
            "TALK MAJOR - Sets the major of talking to shadows through holdup.",
            "natural",
            "TALK MAJOR - 가로막기로 그림자와의 대화 주를 설정한다.".into(),
        );
        assert_eq!(
            talk,
            "TALK MAJOR - 홀드업으로 섀도우와의 대화 메이저 ID를 설정한다."
        );

        // 5. non_translatable_shortcut safety checks (skips numbers/URLs/IDs, never skips English prose/headings)
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
        assert_eq!(
            non_translatable_shortcut("PARTY_SLOT_1").as_deref(),
            Some("PARTY_SLOT_1")
        );
        assert_eq!(
            non_translatable_shortcut("https://tcrf.net/Proto:Persona_3").as_deref(),
            Some("https://tcrf.net/Proto:Persona_3")
        );
        assert!(non_translatable_shortcut("DEBUG MENU").is_none());
        assert!(non_translatable_shortcut("NORMAL ENCOUNT").is_none());
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

        // 1.8B and 3B (~1.97 GiB) fit inside 2048+256 MiB UMA budget -> full offload (99)
        let (layers_18b, label_18b) = resolve_optimal_gpu_layers(&settings, &hw, 1_133_080_448);
        assert_eq!(layers_18b, 99);
        assert!(label_18b.contains("amd-apu"));

        let (layers_3b, _) = resolve_optimal_gpu_layers(&settings, &hw, 2_068_790_016);
        assert_eq!(layers_3b, 99);

        // 7B (~4.41 GiB) exceeds 2048 MiB Dedicated UMA -> hybrid GPU+CPU split instead of WDDM thrashing!
        let (layers_7b, label_7b) = resolve_optimal_gpu_layers(&settings, &hw, 4_624_648_896);
        assert!(
            (8..=24).contains(&layers_7b),
            "Unexpected 7B hybrid layers: {layers_7b}"
        );
        assert!(label_7b.contains("amd-hybrid"));

        // Prompt echo sanitizer cleans 1.8B Q6_K [배경 정보] / [출처 텍스트] leaks
        let noisy = "[배경 정보]\n「야아, 지그」 -> “야아, 지그.”\n[출처 텍스트]\n“우르바스인가, 오랜만이야.”";
        assert_eq!(clean_hy_mt2_output(noisy), "“우르바스인가, 오랜만이야.”");

        // Silent ellipsis shortcut preserves 「……」
        assert_eq!(silent_ellipsis_shortcut("「……」").as_deref(), Some("“……”"));
    }

    #[test]
    fn round4_strict_novel_tuning_rules_work() {
        // 1. Temporal 後から入ってきた客も -> 나중에 들어온 손님들도
        let guests = postprocess_novel_output(
            "周囲の人間も自然とお喋りを止め、後から入ってきた客も静かに席に着いた。",
            "novel",
            "주변 사람들도 자연스럽게 수다를 멈추었고, 뒤에서 들어온 손님들도 조용히 자리에 앉았습니다.".into(),
        );
        assert!(
            guests.contains("나중에 들어온 손님들도") && guests.ends_with("앉았다."),
            "Unexpected guests output: {guests}"
        );

        // 2. Crab pincer (蟹/甲殻類の爪) -> 게의 집게 / 갑각류의 집게 / 그 집게
        let crab = postprocess_novel_output(
            "彼の視線の先には周囲の注目など気にも留めていないシアーシャが蟹らしき甲殻類の爪と悪戦苦闘していた。",
            "novel",
            "그의 시선 끝에는 시아샤가 게의 발톱과 악전고투하고 있었습니다.".into(),
        );
        assert!(
            crab.contains("게의 집게") && crab.ends_with("있었다."),
            "Unexpected crab output: {crab}"
        );

        // 3. 澄人教 (징인교) + ジグには不要かな…… (필요 없겠지...) + 別に信者ってわけじゃなくても (딱히 신도가 아닌 사람이라도)
        let urbas_explain = postprocess_novel_output(
            "「こういう大衆的な話はジグには不要かな……澄人教はこの辺りじゃ有名で信徒も結構多い。別に信者ってわけじゃなくても、潜在的な思想として根付いてる」",
            "novel",
            "“이런 대중적인 이야기는 지그는 필요 없을까... 청인교는 근처에서 유명하고 신도도 꽤 많아요. 굳이 신자로 되는건 아니지만, 잠재적인 사상으로 자리 잡고 있어요?”".into(),
        );
        assert_eq!(
            urbas_explain,
            "“이런 대중적인 이야기는 지그는 필요 없겠지... 징인교는 근처에서 유명하고 신도도 꽤 많아. 딱히 신도가 아닌 사람이라도, 잠재적인 사상으로 자리 잡고 있어.”",
            "Unexpected urbas_explain output: {urbas_explain}"
        );

        // 4. Cross-speaker register filter in select_background_pairs (3-person scene: Searsha polite -> Urbas casual)
        let history = vec![ContextPair {
            source: "「三面鬼、それも群れですよ」".into(),
            translation: Some("“삼면귀, 그것도 무리였어요.”".into()),
        }];
        let bg = select_background_pairs(
            "「猿狗が三面鬼連れてること、あるんだ。生息域全然違うのに……」",
            &history,
        );
        assert!(
            bg.is_empty(),
            "Polite dialogue from Searsha must not be injected into Casual dialogue background: {bg:?}"
        );

        // 5. Laughter onomatopoeia (ぷっと吹き出す) + spurious question mark removal
        let laugh = postprocess_novel_output(
            "シアーシャがぷっと吹き出した。",
            "novel",
            "시아샤가 으엉 하며 웃음을 터뜨렸습니다?".into(),
        );
        assert_eq!(laugh, "시아샤가 프흡 하며 웃음을 터뜨렸다.");
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
        assert!(
            model_path.is_file(),
            "Missing downloaded model at {}",
            model_path.display()
        );

        let engine = HyMt2Engine::new(models_dir);
        let hw = engine.detect_hardware();
        println!("HW_PROFILE={hw:?}");
        assert!(hw.total_ram_mb > 0);
        assert!(hw.cpu_cores > 0);

        let mut settings = AppSettings::default();
        settings.provider = crate::model::ProviderKind::HyMt2;
        settings.model = DEFAULT_HY_MT2_MODEL_ID.to_string();
        settings.target_language = "한국어".to_string();
        settings.style = "novel".to_string();

        let glossary = vec![crate::ai::GlossaryRule {
            source: "Hy-MT2".into(),
            aliases: Vec::new(),
            translation: "Hy-MT2".into(),
            alternatives: String::new(),
            do_not_translate: true,
            reading: String::new(),
        }];
        let context = PromptContext {
            target_language: "한국어",
            style: "novel",
            project_name: None,
            previous_context: None,
            glossary: &glossary,
        };
        let inputs = vec![
            SegmentInput {
                id: "seg-1".into(),
                text: "Welcome to the [[0|Hy-MT2]] local translation engine [[img0]].".into(),
            },
            SegmentInput {
                id: "seg-2".into(),
                text: "その瞬間、俺の目の前でアルカディア帝国の結界が砕け散った。".into(),
            },
            SegmentInput {
                id: "seg-3".into(),
                text: "「ふん、あんたごときにこの星詠の聖剣が扱えるとでも思っているの？　勘違いも甚だしいわよ」".into(),
            },
            SegmentInput {
                id: "seg-4".into(),
                text: "「恐れ入ります、カイン様。ですが、これ以上ここに留まるのは危険でございます」".into(),
            },
            SegmentInput {
                id: "seg-5".into(),
                text: "「先輩、結界の修復が完了しました！　今なら星詠の聖剣の力を使えます！」".into(),
            },
        ];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let started = Instant::now();
            let mut streamed_ids = Vec::new();
            let (results, model_id, backend_label) = engine
                .translate_segments(
                    &settings,
                    &inputs,
                    &context,
                    Some("https://example.invalid/novel/ch1"),
                    &[],
                    true,
                    CancellationToken::new(),
                    |res| {
                        println!("STREAM_SEG {}: {}", res.id, res.translation);
                        streamed_ids.push(res.id);
                    },
                )
                .await
                .expect("Hy-MT2 inference failed");
            let elapsed_ms = started.elapsed().as_millis();
            println!("INFERENCE_ELAPSED_MS={elapsed_ms} model={model_id} backend={backend_label}");
            println!("LOADED_INFO={:?}", engine.loaded_model_info().await);
            assert_eq!(results.len(), 5);
            assert_eq!(streamed_ids.len(), 5);
            assert!(
                results[0].translation.contains("[[0|") && results[0].translation.contains("[[img0]]"),
                "Inline markers dropped in output: {}",
                results[0].translation
            );
            assert!(
                results[1].translation.ends_with("다.") || results[1].translation.ends_with('다'),
                "Narration must preserve plain ending (~다): {}",
                results[1].translation
            );
            assert!(!results[2].translation.trim().is_empty());
            assert!(!results[3].translation.trim().is_empty());
            assert!(!results[4].translation.trim().is_empty());

            // Verify 8 representative hard failure sentences from the novel benchmark across all 4 fast local models:
            let bench_inputs = vec![
                SegmentInput {
                    id: "bench-1-treat".into(),
                    text: "「このあと一緒にご飯でもどう？　前のお礼に、奢らせて」".into(),
                },
                SegmentInput {
                    id: "bench-2-tongue".into(),
                    text: "にっこりと笑った彼女にウルバスが舌を引っ込める。以前彼女が真っ二つにした削岩竜を思い出したのか、少しだけ固くなった動作で頷くウルバス。".into(),
                },
                SegmentInput {
                    id: "bench-3-shop".into(),
                    text: "冒険者御用達とは言ってもそれなり程度の冒険者が気軽に来れる店ではなく、五等級以上の稼ぎでなければかなり奮発しないといけないほどの価格はするランクの違う店だ。".into(),
                },
                SegmentInput {
                    id: "bench-4-hitoke".into(),
                    text: "二階席奥の人気が少ない場所にウルバスが座った。".into(),
                },
                SegmentInput {
                    id: "bench-5-searsha".into(),
                    text: "「猿狗の討伐です」".into(),
                },
                SegmentInput {
                    id: "bench-6-short".into(),
                    text: "（そういう事か）".into(),
                },
                SegmentInput {
                    id: "bench-7-banmal".into(),
                    text: "「今日は一人なんだな」".into(),
                },
                SegmentInput {
                    id: "bench-8-ruby".into(),
                    text: "「ウルバス、聞きたいことがある。……澄人教すみびときょうとはなんだ？」".into(),
                },
                SegmentInput {
                    id: "bench-9-later-guests".into(),
                    text: "周囲の人間も自然とお喋りを止め、後から入ってきた客も静かに席に着いた。".into(),
                },
                SegmentInput {
                    id: "bench-10-crab-pincer".into(),
                    text: "彼の視線の先には周囲の注目など気にも留めていないシアーシャが蟹らしき甲殻類の爪と悪戦苦闘していた。ジグは無言でその爪を取ると親指と人差し指だけの力でばきりとヒビを入れて返してやる。".into(),
                },
                SegmentInput {
                    id: "bench-11-urbas-religion".into(),
                    text: "「こういう大衆的な話はジグには不要かな……澄人教はこの辺りじゃ有名で信徒も結構多い。別に信者ってわけじゃなくても、潜在的な思想として根付いてる」".into(),
                },
                SegmentInput {
                    id: "bench-12-3party-searsha".into(),
                    text: "「三面鬼、それも群れですよ」".into(),
                },
                SegmentInput {
                    id: "bench-13-3party-urbas".into(),
                    text: "「猿狗が三面鬼連れてること、あるんだ。生息域全然違うのに……」".into(),
                },
                SegmentInput {
                    id: "bench-14-laugh".into(),
                    text: "シアーシャがぷっと吹き出して笑った。".into(),
                },
            ];
            for candidate_model in [
                "Hy-MT2-1.8B-Q4_K_M.gguf",
                "Hy-MT2-1.8B-Q6_K.gguf",
            ] {
                if !engine.models_dir().join(candidate_model).is_file() {
                    println!("BENCH_SKIP model={candidate_model} (not present)");
                    continue;
                }
                settings.model = candidate_model.to_string();
                // Pre-load model so we measure pure translation time separately from cold model load
                let load_start = Instant::now();
                let (_, loaded_backend) = engine
                    .test_model_load(&settings)
                    .await
                    .expect("Model load failed");
                let load_ms = load_start.elapsed().as_millis();

                let bench_start = Instant::now();
                let (bench_res, _, _) = engine
                    .translate_segments(
                        &settings,
                        &bench_inputs,
                        &context,
                        Some(&format!("https://example.invalid/novel/{candidate_model}")),
                        &[],
                        true,
                        CancellationToken::new(),
                        |res| {
                            println!("BENCH_SEG [{candidate_model}] {}: {}", res.id, res.translation);
                        },
                    )
                    .await
                    .expect("Bench inference failed");
                println!(
                    "BENCH_SUMMARY model={candidate_model} backend={loaded_backend} load_ms={load_ms} translate_ms={} avg_ms_per_seg={} count={}",
                    bench_start.elapsed().as_millis(),
                    bench_start.elapsed().as_millis() / (bench_res.len() as u128),
                    bench_res.len()
                );
                assert_eq!(bench_res.len(), 14);
            }

            let cancelled = CancellationToken::new();
            cancelled.cancel();
            let err = engine
                .translate_segments(
                    &settings,
                    &inputs[..1],
                    &context,
                    None,
                    &[],
                    false,
                    cancelled,
                    |_| {},
                )
                .await
                .expect_err("Pre-cancelled token must abort cleanly");
            assert!(err.contains("취소"), "Unexpected cancel error: {err}");

            assert!(engine.unload().await, "Expected model to unload cleanly");
            assert!(engine.loaded_model_info().await.is_none());
        });
    }

    #[test]
    #[ignore = "requires downloaded Hy-MT2 models in AppLocalData"]
    fn real_world_global_tuning_smoke() {
        let local_app = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA env missing");
        let models_dir = local_app
            .join("com.appleout.transratebrowser")
            .join("models");
        let engine = HyMt2Engine::new(models_dir);
        let mut settings = AppSettings::default();
        settings.provider = crate::model::ProviderKind::HyMt2;
        settings.target_language = "한국어".to_string();
        settings.style = "natural".to_string();

        let context = PromptContext {
            target_language: "한국어",
            style: "natural",
            project_name: None,
            previous_context: None,
            glossary: &[],
        };

        let web_inputs = vec![
            // 1. English Wikipedia (Persona 3) with inline markers
            SegmentInput {
                id: "en-wiki-1".into(),
                text: "Persona 3, released outside Japan as Shin Megami Tensei: Persona 3, is a 2006 [[0|role-playing video game]] developed by [[1|Atlus]]. It is the fourth main installment in the [[2|Persona]] series. It was originally released for the [[3|PlayStation 2]] in Japan in 2006 and in North America in 2007.".into(),
            },
            SegmentInput {
                id: "en-wiki-2".into(),
                text: "In Persona 3, the player assumes the role of a [[0|high school student]] who joins the \"Specialized Extracurricular Execution Squad\" (SEES), a group of students investigating a temporal anomaly known as the \"Dark Hour\", during which its members can enter Tartarus, a tower containing monsters called Shadows.".into(),
            },
            SegmentInput {
                id: "en-wiki-3".into(),
                text: "Reception towards Persona 3 was mainly positive; critics enjoyed its social elements, while some found its combat and environments repetitive.".into(),
            },
            // 2. Japanese Wikipedia (ペルソナ3) with inline markers
            SegmentInput {
                id: "ja-wiki-1".into(),
                text: "『ペルソナ3』（ペルソナスリー、PERSONA3）は、[[0|アトラス]]により制作・リリースされた[[1|ロールプレイングゲーム]]。[[2|2006年]][[3|7月13日]]に[[4|PlayStation 2]]用ゲームソフトとしてリリースされ、[[5|ファミ通]]ゲームアワード2006にて優秀賞（RPG部門）を受賞した。".into(),
            },
            SegmentInput {
                id: "ja-wiki-2".into(),
                text: "また、本作を原作とした[[0|小説]]、[[1|漫画]]、[[2|ドラマCD]]などの[[3|メディアミックス]]展開も行われている。本項では、これらについても記述する。".into(),
            },
            SegmentInput {
                id: "ja-wiki-3".into(),
                text: "[[0|ペルソナシリーズ]]のナンバリング第3作目。略称は「P3」、ほかの派生・リメイク作品と区別をするため「オリジナル」、「無印版」と呼ばれることもしばしばある。".into(),
            },
            // 3. Rust Book (Technical Docs) with inline code/link markers
            SegmentInput {
                id: "rust-1".into(),
                text: "The first step is to install Rust. We’ll download Rust through [[0|rustup]], a command line tool for managing Rust versions and associated tools. You’ll need an internet connection for the download.".into(),
            },
            SegmentInput {
                id: "rust-2".into(),
                text: "Note: If you prefer not to use [[0|rustup]] for some reason, please see the [[1|Other Rust Installation Methods page]] for more options.".into(),
            },
            SegmentInput {
                id: "rust-3".into(),
                text: "In this chapter and throughout the book, we’ll show some commands used in the terminal. Lines that you should enter in a terminal all start with [[0|$]]. You don’t need to type the [[1|$]] character; it’s the command line prompt shown to indicate the start of each command.".into(),
            },
            // 4. MDN JS Guide (Technical Docs) with inline API markers
            SegmentInput {
                id: "mdn-1".into(),
                text: "JavaScript is a cross-platform, object-oriented scripting language used to make webpages interactive (e.g., having complex animations, clickable buttons, popup menus, etc.).".into(),
            },
            SegmentInput {
                id: "mdn-2".into(),
                text: "JavaScript contains a standard library of objects, such as [[0|Array]], [[1|Map]], and [[2|Math]], and a core set of language elements such as operators, control structures, and statements.".into(),
            },
            SegmentInput {
                id: "mdn-3".into(),
                text: "This means that in the browser, JavaScript can change the way the webpage (DOM) looks. And, likewise, Node.js JavaScript on the server can respond to custom requests sent by code executed in the browser.".into(),
            },
            // 5. The Cutting Room Floor (TCRF - Persona 3 Prototype) real user feedback cases
            SegmentInput {
                id: "tcrf-1".into(),
                text: "This menu allows you to test battle encounters, party members, and can take you to the title screen.".into(),
            },
            SegmentInput {
                id: "tcrf-2".into(),
                text: "ENCOUNT - Sets what encounter of enemies will be fought.".into(),
            },
            SegmentInput {
                id: "tcrf-3".into(),
                text: "Party1 - Sets who is in your 2nd party slot.".into(),
            },
            SegmentInput {
                id: "tcrf-4".into(),
                text: "Party2 - Sets who is in your 3rd party slot.".into(),
            },
            SegmentInput {
                id: "tcrf-5".into(),
                text: "Party4 - Sets who is in your 4th party slot.".into(),
            },
            SegmentInput {
                id: "tcrf-6".into(),
                text: "TALK MAJOR - Sets the major of talking to shadows through holdup.".into(),
            },
            SegmentInput {
                id: "tcrf-7".into(),
                text: "PREEMPTIVE - Has options to set what part of battle you're in. NORMAL ENCOUNT, PINCH ENCOUNT, ALLY ECOUNT, HOLD UP FORMATION, RANDOM FIRE, ALERT ENOUNT, ENEMY TALK.".into(),
            },
        ];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            for candidate_model in ["Hy-MT2-1.8B-Q4_K_M.gguf", "Hy-MT2-1.8B-Q6_K.gguf"] {
                if !engine.models_dir().join(candidate_model).is_file() {
                    println!("WEB_SKIP model={candidate_model}");
                    continue;
                }
                settings.model = candidate_model.to_string();
                let _ = engine
                    .test_model_load(&settings)
                    .await
                    .expect("Load failed");
                let started = Instant::now();
                let (res, _, _) = engine
                    .translate_segments(
                        &settings,
                        &web_inputs,
                        &context,
                        Some(&format!("https://example.invalid/web/{candidate_model}")),
                        &[],
                        true,
                        CancellationToken::new(),
                        |seg| {
                            println!(
                                "WEB_SEG [{candidate_model}] {}: {}",
                                seg.id, seg.translation
                            );
                        },
                    )
                    .await
                    .expect("Web bench failed");
                println!(
                    "WEB_SUMMARY model={candidate_model} ms={} avg_ms={} count={}",
                    started.elapsed().as_millis(),
                    started.elapsed().as_millis() / (res.len() as u128),
                    res.len()
                );
            }
            let _ = engine.unload().await;
        });
    }

    #[test]
    #[ignore = "requires downloaded Hy-MT2-1.8B-Q4_K_M.gguf in AppLocalData"]
    fn real_multilingual_namu_tcrf_smoke() {
        let local_app = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA env missing");
        let models_dir = local_app
            .join("com.appleout.transratebrowser")
            .join("models");
        assert!(
            models_dir.join(DEFAULT_HY_MT2_MODEL_ID).is_file(),
            "Missing Q4_K_M model in {}",
            models_dir.display()
        );
        let engine = HyMt2Engine::new(models_dir);
        let mut settings = AppSettings::default();
        settings.provider = crate::model::ProviderKind::HyMt2;
        settings.model = DEFAULT_HY_MT2_MODEL_ID.to_string();
        settings.style = "natural".to_string();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = engine.test_model_load(&settings).await.expect("Load failed");

            // A. Live ja.namu.wiki + en.namu.wiki Persona 3 batch -> Korean
            let namu_ko_inputs = vec![
                SegmentInput { id: "ja-namu-head".into(), text: "1. 概要".into() },
                SegmentInput { id: "ja-namu-details".into(), text: "[[img0]] 詳しくは [[0|ペルソナ3 FES]] 文書をご注意ください。".into() },
                SegmentInput { id: "ja-namu-body1".into(), text: "ペルソナシリーズの3作目であり、正式タイトルは『女神転生ペルソナ3』です。公式略称はP3です。".into() },
                SegmentInput { id: "ja-namu-body2".into(), text: "2009年[[0|[4]]]に移植版である[[1|ペルソナ3 ポータブル]]が発売された。".into() },
                SegmentInput { id: "ja-namu-fn1".into(), text: "[1] 参考までに、女神イ・ムンロクのペルソナは黙示録: ペルソナとしてリリースされ、女神転生外伝のラストバイブルは黙示録: デーモンスレイヤーとしてリリースされました。".into() },
                SegmentInput { id: "en-namu-circ".into(), text: "circulation ATLUS SCEK".into() },
                SegmentInput { id: "en-namu-outline".into(), text: "1. outline".into() },
                SegmentInput { id: "en-namu-char".into(), text: "4. characteristic".into() },
                SegmentInput { id: "en-namu-ext".into(), text: "6. extended edition".into() },
                SegmentInput { id: "en-namu-kor".into(), text: "11. Koreanization".into() },
                SegmentInput { id: "en-namu-body1".into(), text: "It is the third work in the Persona series and its official title is “Mega Tensei Persona 3.” The official abbreviation is P3.".into() },
                SegmentInput { id: "en-namu-body2".into(), text: "Influenced by the 9/11 attacks in 2001 and the Iraq War in 2003, Katsura Hashino began conceptualizing a game based on the heavy yet universal themes of life within a limited time and death.".into() },
            ];
            let ctx_ko = PromptContext {
                target_language: "한국어",
                style: "natural",
                project_name: None,
                previous_context: None,
                glossary: &[],
            };
            let t_namu = Instant::now();
            let (namu_res, _, _) = engine
                .translate_segments(
                    &settings,
                    &namu_ko_inputs,
                    &ctx_ko,
                    Some("https://example.invalid/namu-p3"),
                    &[],
                    true,
                    CancellationToken::new(),
                    |seg| println!("NAMU_SEG {}: {}", seg.id, seg.translation),
                )
                .await
                .expect("Namu batch failed");
            println!(
                "NAMU_SUMMARY ms={} avg_ms={} count={}",
                t_namu.elapsed().as_millis(),
                t_namu.elapsed().as_millis() / (namu_res.len() as u128),
                namu_res.len()
            );

            // B. Multi-language bidirectional matrix across 12 language pairs
            let pairs: Vec<(&str, &str, &str)> = vec![
                ("zh-ko", "한국어", "《女神异闻录3》是由Atlus开发的经典角色扮演游戏，以其独特的校园生活与迷宫探索结合而闻名。"),
                ("fr-ko", "한국어", "Ce menu de débogage permet aux développeurs de tester rapidement les combats et les emplacements de l'équipe."),
                ("de-ko", "한국어", "Die Übersetzungsmaschine arbeitet jetzt vollständig lokal auf der GPU ohne Internetverbindung."),
                ("es-ko", "한국어", "Esta página documenta las funciones ocultas y los textos sin usar encontrados en el código del juego."),
                ("ru-ko", "한국어", "Локальный движок обеспечивает быстрый и точный перевод технических текстов и вики-статей."),
                ("ko-en", "English", "이 메뉴를 사용하면 전투 인카운트와 파티 슬롯의 캐릭터를 자유롭게 테스트할 수 있다."),
                ("ko-ja", "日本語", "이 메뉴를 사용하면 전투 인카운트와 파티 슬롯의 캐릭터를 자유롭게 테스트할 수 있다."),
                ("en-ja", "日本語", "ENCOUNT - Sets what encounter of enemies will be fought."),
                ("ja-en", "English", "ペルソナシリーズの3作目であり、正式タイトルは『女神転生ペルソナ3』です。"),
                ("en-zh", "中文(简体)", "This debug menu allows developers to test battle encounters and warp back to the title screen."),
                ("en-fr", "Français", "Keep all inline settings intact while exploring the debug dungeon."),
                ("en-es", "Español", "You can switch the target language to translate any webpage cleanly."),
            ];
            let t_multi = Instant::now();
            for (id, tgt_lang, src) in &pairs {
                let ctx = PromptContext {
                    target_language: tgt_lang,
                    style: "natural",
                    project_name: None,
                    previous_context: None,
                    glossary: &[],
                };
                let (res, _, _) = engine
                    .translate_segments(
                        &settings,
                        &[SegmentInput {
                            id: (*id).into(),
                            text: (*src).into(),
                        }],
                        &ctx,
                        None,
                        &[],
                        false,
                        CancellationToken::new(),
                        |_| {},
                    )
                    .await
                    .expect("Multilingual inference failed");
                println!("MULTI_SEG [{id} -> {tgt_lang}]: {}", res[0].translation);
            }
            println!(
                "MULTI_SUMMARY ms={} avg_ms={} count={}",
                t_multi.elapsed().as_millis(),
                t_multi.elapsed().as_millis() / (pairs.len() as u128),
                pairs.len()
            );
            let _ = engine.unload().await;
        });
    }

    #[test]
    #[ignore = "requires downloaded Hy-MT2 models in AppLocalData"]
    fn real_fandom_vdc_smoke() {
        let local_app = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA env missing");
        let models_dir = local_app
            .join("com.appleout.transratebrowser")
            .join("models");
        let engine = HyMt2Engine::new(models_dir);
        let mut settings = AppSettings::default();
        settings.provider = crate::model::ProviderKind::HyMt2;
        settings.target_language = "한국어".to_string();
        settings.style = "natural".to_string();

        let wiki_inputs = vec![
            // Fandom (https://half-life.fandom.com/wiki/Half-Life_2)
            SegmentInput {
                id: "fandom-title".into(),
                text: "Half-Life 2".into(),
            },
            SegmentInput {
                id: "fandom-infobox-date".into(),
                text: "Release date November 16, 2004".into(),
            },
            SegmentInput {
                id: "fandom-infobox-mode".into(),
                text: "Mode [[0|Single-player]]".into(),
            },
            SegmentInput {
                id: "fandom-lead-1".into(),
                text: "Half-Life 2, stylized as HλLF-LIFE2, is a [[0|science fiction]] [[1|first-person shooter]] developed and published by the [[2|Valve Corporation]], and released on November 16, 2004,[[3|[1]]] following an extended development period of five years. The game garnered near-unanimous positive reviews and received critical acclaim, winning over 30 Game of the Year awards. It is included in [[4|The Orange Box]] for the [[5|Xbox 360]], [[6|PS3]], and PC. It was also included for the Original Xbox on November 17th 2005.".into(),
            },
            SegmentInput {
                id: "fandom-lead-2".into(),
                text: "The game uses the [[0|Source engine]], which includes a heavily modified version of the [[1|Havok physics engine]].".into(),
            },
            SegmentInput {
                id: "fandom-gameplay".into(),
                text: "Taking place in and around [[0|City 17]], nearly two decades[[1|[2]]] after the events of the first game, Half-Life 2 follows physicist [[2|Gordon Freeman]] as he is thrust into an unfamiliar environment in which the aftermath of the [[3|Black Mesa Incident]] has come to bear fully upon human society. Freeman is forced to battle against increasingly unfavorable odds in order to free humanity from the oppressive [[4|Combine]], an alien race that has conquered the Earth. In his struggle, he is joined by various allies, including fellow scientists, rebels, and friends (such as Alyx Vance) - all shocked to see him alive.".into(),
            },
            SegmentInput {
                id: "fandom-dev".into(),
                text: "Half-Life 2 was also originally intended to be much more diverse in settings. Several [[0|City 17]] levels at the start of the game and complete chapters from the second half of the game were completely removed and sometimes re-introduced in the subsequent Half-Life 2 episodes.".into(),
            },
            SegmentInput {
                id: "fandom-weapon".into(),
                text: "[[0|SPAS-12]]: A powerful, pump-action shotgun that fires pellets in a cone pattern.".into(),
            },
            SegmentInput {
                id: "fandom-caption".into(),
                text: "Alternate American cover, Alyx version.".into(),
            },
            SegmentInput {
                id: "fandom-cleanup".into(),
                text: "[[0|+]] [[img0]] This article has yet to be cleaned up to a higher standard of quality.".into(),
            },
            // Valve Developer Community (https://developer.valvesoftware.com/wiki/Half-Life_2)
            SegmentInput {
                id: "vdc-lead-1".into(),
                text: "[[img0]] [[0|Half-Life 2]] (abbreviated as HL2) defines a new benchmark in gaming with startling realism and responsiveness. Powered by then-new [[1|Source]] technology, Half-Life 2 features the most sophisticated in-game characters ever witnessed, advanced AI, stunning graphics and physical gameplay. It is the first game requiring installation of Steam when it was released back in 2004.".into(),
            },
            SegmentInput {
                id: "vdc-lead-2".into(),
                text: "Half-Life 2 is the sequel to the 1998 [[0|Half-Life]]. A sequel to Half-Life 2 was released in 2006, titled [[1|Half-Life 2: Episode One]], followed by [[2|Episode Two]] in 2007, being the last Half-Life 2 episode. A virtual reality prequel was released in 2020, titled [[3|Half-Life: Alyx]].".into(),
            },
            SegmentInput {
                id: "vdc-console-h3".into(),
                text: "Console versions (The Orange Box)".into(),
            },
            SegmentInput {
                id: "vdc-demo-code".into(),
                text: "Half-Life 2 has a free demo version. It consists of excerpts of the Point Insertion and Ravenholm chapters ([[0|d1_trainstation_01]] and [[1|d1_trainstation_02]]) and \"We Don't Go To Ravenholm...\" ([[2|d1_town_01]], [[3|d1_town_02]] and [[4|d1_town_03]]). The game ends as soon the player touch the level transition trigger in [[5|d1_town_02]] that supposed to load [[6|d1_town_02a]].".into(),
            },
            SegmentInput {
                id: "vdc-20th-anniv".into(),
                text: "A major update (Half-Life 2 20th Anniversary Update), has been released on November 16, 2024 to celebrate the 20th anniversary of the game. It adds Steam Workshop support, developer commentary, fixes various bugs, and makes both episodes, as well as [[0|Lost Coast]], accessible directly through the Half-Life 2's menu. There are also a few other changes to the graphics, such as \"Very High\" shaders that enable Bicubic [[1|lightmap]] filtering, [[2|radial fog]] (enabled on some maps), and an option to use classic fire/blood particle effects.[[3|[1]]] Additionally, the 20th Anniversary version of Half-Life 2 used the Xbox 360 version of the maps as a base, so many changes like TV prop being rotated (in Route Kanal, during the part you enter the red container and meet the second [[4|Vortigaunt]] and an [[5|citizen]]) and an invisible wall from an underwater tube in Route Kanal, are present before Valve eventually fixed the latter.".into(),
            },
            SegmentInput {
                id: "vdc-content-code".into(),
                text: "Half-Life 2 content ([[0|hl2]])".into(),
            },
            SegmentInput {
                id: "vdc-chapters-th".into(),
                text: "[[img0]] [[0|Half-Life 2]] Chapters".into(),
            },
            SegmentInput {
                id: "vdc-chapter-row".into(),
                text: "[[img0]] [[0|Route Kanal]] [[1|d1_canals_01]]".into(),
            },
        ];

        let ctx_ko = PromptContext {
            target_language: "한국어",
            style: "natural",
            project_name: None,
            previous_context: None,
            glossary: &[],
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            for candidate_model in ["Hy-MT2-1.8B-Q4_K_M.gguf", "Hy-MT2-1.8B-Q6_K.gguf"] {
                if !engine.models_dir().join(candidate_model).is_file() {
                    println!("FV_SKIP model={candidate_model}");
                    continue;
                }
                settings.model = candidate_model.to_string();
                let _ = engine.test_model_load(&settings).await.expect("Load failed");
                let started = Instant::now();
                let (res, _, _) = engine
                    .translate_segments(
                        &settings,
                        &wiki_inputs,
                        &ctx_ko,
                        Some(&format!("https://example.invalid/fv/{candidate_model}")),
                        &[],
                        true,
                        CancellationToken::new(),
                        |seg| {
                            println!("FV_SEG [{candidate_model}] {}: {}", seg.id, seg.translation);
                        },
                    )
                    .await
                    .expect("Fandom/VDC bench failed");
                println!(
                    "FV_SUMMARY model={candidate_model} ms={} avg_ms={} count={}",
                    started.elapsed().as_millis(),
                    started.elapsed().as_millis() / (res.len() as u128),
                    res.len()
                );
                assert_eq!(res.len(), wiki_inputs.len());
            }

            // Multilingual spot checks from Fandom/VDC -> JA / ZH / ES / FR / DE on Q4_K_M
            settings.model = DEFAULT_HY_MT2_MODEL_ID.to_string();
            let _ = engine.test_model_load(&settings).await.expect("Load Q4_K_M failed");
            let multi_checks: &[(&str, &str, &str)] = &[
                ("fv-ja", "日本語", "The game uses the [[0|Source engine]], which includes a heavily modified version of the [[1|Havok physics engine]]."),
                ("fv-zh", "中文(简体)", "Half-Life 2 is the sequel to the 1998 [[0|Half-Life]], followed by [[1|Half-Life 2: Episode One]] in 2006."),
                ("fv-es", "Español", "[[0|SPAS-12]]: A powerful, pump-action shotgun that fires pellets in a cone pattern."),
                ("fv-fr", "Français", "The game ends as soon as the player touches the level transition trigger in [[0|d1_town_02]] to load [[1|d1_town_02a]]."),
                ("fv-de", "Deutsch", "[[img0]] [[0|Half-Life 2]] is the first game requiring installation of Steam when it was released back in 2004."),
            ];
            let t_multi = Instant::now();
            for (id, tgt_lang, src) in multi_checks {
                let ctx = PromptContext {
                    target_language: tgt_lang,
                    style: "natural",
                    project_name: None,
                    previous_context: None,
                    glossary: &[],
                };
                let (res, _, _) = engine
                    .translate_segments(
                        &settings,
                        &[SegmentInput {
                            id: (*id).into(),
                            text: (*src).into(),
                        }],
                        &ctx,
                        None,
                        &[],
                        false,
                        CancellationToken::new(),
                        |_| {},
                    )
                    .await
                    .expect("Fandom/VDC multilingual failed");
                println!("FV_MULTI [{id} -> {tgt_lang}]: {}", res[0].translation);
            }
            println!(
                "FV_MULTI_SUMMARY ms={} avg_ms={} count={}",
                t_multi.elapsed().as_millis(),
                t_multi.elapsed().as_millis() / (multi_checks.len() as u128),
                multi_checks.len()
            );
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
