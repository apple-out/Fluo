#ifndef FLUO_IOS_CORE_H
#define FLUO_IOS_CORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Loads an on-device GGUF model (e.g., Hy-MT2-1.8B) into memory with Metal GPU offload.
/// Returns 0 on success, non-zero on failure.
int32_t fluo_load_model(const char *model_path, int32_t n_gpu_layers, uint32_t n_ctx);

/// Returns last model load error string (must be freed with fluo_free_string()).
char *fluo_last_error(void);

/// Translates a single text segment into target_lang ("Korean", "English", "Japanese", "Chinese").
/// Returns a newly allocated UTF-8 JSON string: {"ok":true,"translation":"..."} or {"ok":false,"error":"..."}.
/// Caller MUST free the returned pointer with fluo_free_string().
char *fluo_translate_text(const char *text, const char *target_lang, const char *style);

/// Same as fluo_translate_text, with optional glossary rules text (one per line, e.g. "Source -> Target").
/// Caller MUST free the returned pointer with fluo_free_string().
char *fluo_translate_with_glossary(
    const char *text,
    const char *target_lang,
    const char *style,
    const char *glossary_hint
);

/// Unloads the active GGUF model immediately to free iOS RAM/Metal buffers (call on backgrounding).
void fluo_unload_model(void);

/// Frees a string pointer returned by fluo_translate_text().
void fluo_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* FLUO_IOS_CORE_H */
