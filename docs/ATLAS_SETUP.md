# Atlas Model Setup

Atlas uses a 9B parameter model ([HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive](https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive), Qwen3.5 architecture) to generate concise symbol descriptions for codebase navigation. The model runs locally via Ollama and outputs structured SHORT/LONG description pairs that mcptools stores in the symbol index.

## Prerequisites

- [Ollama](https://ollama.com) installed and running

## Setup

### 1. Download the quantized model

```bash
curl -L -o atlas-9b.Q4_K_M.gguf \
  https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive-GGUF/resolve/main/atlas-9b.Q4_K_M.gguf
```

This downloads the Q4_K_M quantization (~5.5 GB) -- a good balance of speed and quality. Check [HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive-GGUF](https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive-GGUF) for the latest available quantizations and exact filenames, as they may change.

### 2. Import into Ollama

Copy the downloaded `.gguf` file into the `models/atlas/` directory in this repository (next to the Modelfile), then import:

```bash
cp atlas-9b.Q4_K_M.gguf models/atlas/
ollama create atlas -f models/atlas/Modelfile
```

The Modelfile configures:
- **ChatML template** (`<|im_start|>`/`<|im_end|>`) matching the Qwen3.5 chat format
- **No system prompt** -- the system prompt is injected at call time by mcptools, so the model can receive context-specific instructions for each symbol type
- **Generation parameters**: `temperature=0.3` (factual, not creative), `top_k=20`, `top_p=0.9`
- **Stop tokens** for proper generation termination

### 3. Verify

```bash
ollama run atlas "Describe this Rust function: fn add(a: i32, b: i32) -> i32 { a + b }"
```

Expected output (format depends on system prompt, but should be concise):

```
SHORT: Adds two 32-bit integers and returns their sum.
LONG: A pure function that takes two `i32` parameters and returns their arithmetic sum. No error handling or overflow checks.
```

## Troubleshooting

### Model outputs verbose or unstructured text

The Atlas model relies on a system prompt injected at call time to produce the SHORT/LONG format. When testing with `ollama run` directly, the model may produce freeform text since no system prompt is set in the Modelfile. This is expected -- mcptools provides the system prompt during indexing.

If `mcptools atlas index` produces malformed descriptions:

1. Verify you imported with the repository's Modelfile (not a bare `FROM` line)
2. Re-import: `ollama create atlas -f models/atlas/Modelfile`

### Model not found error

If `mcptools atlas index` reports the model is not found, ensure:

1. Ollama is running (`ollama serve`)
2. The model was imported successfully (`ollama list` should show `atlas`)
3. The Ollama URL is correct (default: `http://localhost:11434`, override with `OLLAMA_URL`)

### Out of memory

The 9B model requires more RAM than smaller models. If you encounter OOM errors:

1. Close other applications to free memory
2. Try a smaller quantization (Q3_K_M) if available
3. Ensure you have at least 8 GB of free RAM

## Model Details

| Property | Value |
|----------|-------|
| Base model | Qwen3.5-9B |
| Parameters | ~9B |
| Chat format | ChatML (`<\|im_start\|>` / `<\|im_end\|>`) |
| Quantization | Q4_K_M (recommended) |
| Download size | ~5.5 GB |
| Temperature | 0.3 (factual) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `ATLAS_FILE_MODEL` | `atlas` | Model name for symbol description generation |

## Usage

```bash
mcptools atlas index                  # Build symbol index for current repo
mcptools atlas tree [path]            # Show annotated directory tree
mcptools atlas peek <path>            # Show file summary + symbols
```
