use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn copy_dll_if_newer(src: &Path, dst: &Path) {
    let Ok(src_meta) = fs::metadata(src) else {
        return;
    };
    if let Ok(dst_meta) = fs::metadata(dst) {
        let same_len = src_meta.len() == dst_meta.len();
        let up_to_date = match (src_meta.modified(), dst_meta.modified()) {
            (Ok(src_t), Ok(dst_t)) => dst_t >= src_t,
            _ => false,
        };
        if same_len && up_to_date {
            return;
        }
    }
    if let Err(err) = fs::copy(src, dst) {
        println!(
            "cargo:warning=Failed to copy {} to {}: {}",
            src.display(),
            dst.display(),
            err
        );
    }
}

fn collect_dlls(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        {
            out.push(path);
        }
    }
}

fn copy_universal_backend_dlls() {
    let Ok(cmake_dir) = env::var("DEP_LLAMA_GGML_CMAKE_DIR") else {
        return;
    };
    let Some(llama_out_dir) = Path::new(&cmake_dir).parent().and_then(Path::parent) else {
        return;
    };
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let Some(profile_dir) = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
    else {
        return;
    };

    let mut dlls = Vec::new();
    collect_dlls(&llama_out_dir.join("bin"), &mut dlls);
    collect_dlls(&llama_out_dir.join("lib"), &mut dlls);
    if dlls.is_empty() {
        return;
    }

    let deps_dir = profile_dir.join("deps");
    let _ = fs::create_dir_all(&profile_dir);
    let _ = fs::create_dir_all(&deps_dir);

    for dll in dlls {
        let Some(name) = dll.file_name() else {
            continue;
        };
        copy_dll_if_newer(&dll, &profile_dir.join(name));
        copy_dll_if_newer(&dll, &deps_dir.join(name));
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=GGML_BACKEND_DL");
    println!("cargo:rerun-if-env-changed=GGML_CPU_ALL_VARIANTS");
    println!("cargo:rerun-if-env-changed=TB_FEEDBACK_WEBHOOK_URL");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let vulkan_enabled = env::var_os("CARGO_FEATURE_VULKAN").is_some();
    let universal_cpu_enabled = env::var_os("CARGO_FEATURE_UNIVERSAL_CPU").is_some();

    if target_os == "windows" && target_env == "msvc" && vulkan_enabled && !universal_cpu_enabled {
        // Delay-load vulkan-1.dll on binaries so tester PCs missing a Vulkan ICD/loader still launch cleanly and fall back to CPU.
        println!("cargo:rustc-link-lib=delayimp");
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:vulkan-1.dll");
    }
    if target_os == "windows" && universal_cpu_enabled {
        copy_universal_backend_dlls();
    }
    tauri_build::build()
}
