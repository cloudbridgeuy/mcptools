# GrepRAG Model Setup

GrepRAG uses a fine-tuned 0.6B parameter model ([greprag0/greprag-0.6b](https://huggingface.co/greprag0/greprag-0.6b), Qwen3-0.6B architecture) to generate regex patterns for finding cross-file references in code. The model runs locally via Ollama and outputs one regex pattern per line, which mcptools wraps into full `rg` commands.

## Prerequisites

- [Ollama](https://ollama.com) installed and running

## Setup

### 1. Download the quantized model

```bash
curl -L -o greprag-0.6b.Q4_K_M.gguf \
  https://huggingface.co/mradermacher/greprag-0.6b-GGUF/resolve/main/greprag-0.6b.Q4_K_M.gguf
```

This downloads the Q4_K_M quantization (~397 MB) -- a good balance of speed and quality. Other quantizations are available from [mradermacher/greprag-0.6b-GGUF](https://huggingface.co/mradermacher/greprag-0.6b-GGUF) (Q8_0 at ~700 MB for higher quality).

### 2. Import into Ollama

Copy the downloaded `.gguf` file into the `models/greprag/` directory in this repository (next to the Modelfile), then import:

```bash
cp greprag-0.6b.Q4_K_M.gguf models/greprag/
ollama create greprag -f models/greprag/Modelfile
```

The Modelfile configures:
- **ChatML template** (`<|im_start|>`/`<|im_end|>`) matching the Qwen3 chat format the model was trained on
- **System prompt** instructing the model to output regex patterns
- **Generation parameters** from the model's training config: `temperature=0.6`, `top_k=20`, `top_p=0.95`
- **Stop tokens** for proper generation termination

### 3. Verify

```bash
ollama run greprag "def draw(self):
    return self.cards.pop()
"
```

Expected output: one regex pattern per line, e.g.:

```
def draw
self\.cards
pop\(\)
class.*Card
class.*Deck
cards\.pop
```

## Troubleshooting

### Model outputs thinking tags or non-pattern text

The model is Qwen3-based and supports a "thinking mode" that produces `<think>...</think>` blocks before answering. The Modelfile's system prompt should suppress this, but if you see thinking output:

1. Verify you imported with the repository's Modelfile (not a bare `FROM` line)
2. Re-import: `ollama create greprag -f models/greprag/Modelfile`

The parser treats every non-empty line as a pattern, so extraneous text in the output will produce invalid rg commands. Ensure the Modelfile system prompt is present.

### Model not found error

If `mcptools grep-rag retrieve` reports the model is not found, ensure:

1. Ollama is running (`ollama serve`)
2. The model was imported successfully (`ollama list` should show `greprag`)
3. The Ollama URL is correct (default: `http://localhost:11434`, override with `--ollama-url` or `OLLAMA_URL`)

## Model Details

| Property | Value |
|----------|-------|
| Base model | Qwen3-0.6B |
| Parameters | ~596M |
| Chat format | ChatML (`<\|im_start\|>` / `<\|im_end\|>`) |
| License | Apache 2.0 |
| Quantization | Q4_K_M (recommended) |
| Download size | ~397 MB |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `GREPRAG_MODEL` | `greprag` | Model name for query generation |

## Usage

```bash
mcptools grep-rag retrieve "self.deck.draw()" --repo-path ./my-project
```
