# Atlas Model Setup

Atlas uses a local LLM via Ollama to generate file descriptions for codebase navigation. The default model is [HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive](https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive), a lossless-uncensored fine-tune of Qwen3.5-9B.

## Prerequisites

- [Ollama](https://ollama.com) installed and running (`ollama serve`)

## Quick Start (use an existing model)

If you already have a model in Ollama, skip the full setup and point Atlas at it:

```bash
export ATLAS_FILE_MODEL=qwen2.5:7b    # or any model from `ollama list`
mcptools atlas init
mcptools atlas index
```

Any instruction-following model works. Larger models produce better descriptions but are slower per file.

## Full Setup (dedicated atlas model)

### 1. Download the quantized GGUF

```bash
curl -L -o models/atlas/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive-Q4_K_M.gguf \
  "https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive/resolve/main/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive-Q4_K_M.gguf"
```

This downloads the Q4_K_M quantization (~5.3 GB). Other quantizations are available from the [model page](https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive):

| File | Quant | Size |
|------|-------|------|
| `...-Q4_K_M.gguf` | Q4_K_M | 5.3 GB (recommended) |
| `...-Q6_K.gguf` | Q6_K | 6.9 GB (higher quality) |
| `...-Q8_0.gguf` | Q8_0 | 8.9 GB (near-lossless) |

### 2. Import into Ollama

```bash
ollama create atlas -f models/atlas/Modelfile
```

The Modelfile configures:
- **ChatML template** (`<|im_start|>`/`<|im_end|>`) matching the Qwen3.5 chat format
- **Thinking mode disabled** -- pre-fills an empty `<think>` block so the model skips chain-of-thought reasoning and responds directly
- **No baked-in system prompt** -- mcptools injects context-specific instructions at call time
- **Generation parameters**: `temperature=0.3` (factual, not creative), `top_k=20`, `top_p=0.9`
- **Stop tokens** for proper generation termination

### 3. Verify

```bash
ollama run atlas "Describe this Rust function: fn add(a: i32, b: i32) -> i32 { a + b }"
```

The model should produce concise output. When called by mcptools, a system prompt instructs it to use the structured SHORT/LONG format:

```
SHORT: Adds two 32-bit integers and returns their sum.
LONG: A pure function that takes two `i32` parameters and returns their arithmetic sum. No error handling or overflow checks.
```

## Usage

```bash
mcptools atlas init                   # Create primer (mental model of your codebase)
mcptools atlas index                  # Build full index (symbols + descriptions)
mcptools atlas tree [path]            # Show annotated directory tree
mcptools atlas peek <path>            # Show file summary + symbols
```

## Troubleshooting

### Model not found error

```
Model 'atlas' not found.
```

1. Verify Ollama is running: `ollama serve`
2. Check the model exists: `ollama list` should show `atlas`
3. Re-import if needed: `ollama create atlas -f models/atlas/Modelfile`
4. Or use an existing model: `export ATLAS_FILE_MODEL=qwen2.5:7b`

### Model outputs verbose or unstructured text

The SHORT/LONG format comes from a system prompt mcptools injects at call time. When testing with `ollama run` directly, the model produces freeform text -- this is expected.

If `mcptools atlas index` produces malformed descriptions:
1. Verify you imported with the repository's Modelfile (not a bare `FROM` line)
2. Re-import: `ollama create atlas -f models/atlas/Modelfile`
3. Try a more capable model: `export ATLAS_FILE_MODEL=qwen2.5:14b`

### Out of memory

The 9B model requires ~6-8 GB RAM. If OOM:
1. Try a smaller model: `export ATLAS_FILE_MODEL=qwen2.5:3b`
2. Use a smaller quantization (Q3_K_M) if available
3. Close other applications to free memory

## Model Details

| Property | Value |
|----------|-------|
| Base model | [Qwen3.5-9B](https://huggingface.co/Qwen/Qwen3.5-9B) |
| Fine-tune | [HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive](https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive) |
| Parameters | ~9B dense, 32 layers |
| Architecture | Hybrid Gated DeltaNet + softmax attention (3:1) |
| Context | 262K native (extendable to 1M with YaRN) |
| Chat format | ChatML (`<\|im_start\|>` / `<\|im_end\|>`) |
| Quantization | Q4_K_M (recommended) |
| Download size | ~5.3 GB |
| Temperature | 0.3 (factual) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `ATLAS_FILE_MODEL` | `atlas` | Model name for file descriptions |
