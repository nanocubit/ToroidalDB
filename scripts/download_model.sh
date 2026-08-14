#!/bin/bash
# Script для загрузки модели multilingual-e5-small

set -e

MODEL_DIR="${1:-./models/multilingual-e5-small}"
echo "📥 Downloading multilingual-e5-small model to $MODEL_DIR"

# Создаём директорию
mkdir -p "$MODEL_DIR"

# Проверяем наличие huggingface-cli
if command -v huggingface-cli &> /dev/null; then
    echo "✅ Using huggingface-cli..."
    huggingface-cli download intfloat/multilingual-e5-small \
        --include "onnx/model.onnx" \
        --include "onnx/model_quantized.onnx" \
        --include "vocab.json" \
        --include "merges.txt" \
        --include "tokenizer.json" \
        --include "tokenizer_config.json" \
        --local-dir "$MODEL_DIR"
else
    echo "⚠️  huggingface-cli not found. Installing..."
    pip install huggingface_hub
    
    huggingface-cli download intfloat/multilingual-e5-small \
        --include "onnx/model.onnx" \
        --include "onnx/model_quantized.onnx" \
        --include "vocab.json" \
        --include "merges.txt" \
        --include "tokenizer.json" \
        --include "tokenizer_config.json" \
        --local-dir "$MODEL_DIR"
fi

# Проверяем наличие файлов
if [ -f "$MODEL_DIR/onnx/model.onnx" ]; then
    echo "✅ Model downloaded successfully!"
    echo ""
    echo "📊 Model files:"
    ls -lh "$MODEL_DIR/onnx/"
    echo ""
    echo "🔧 Tokenizer files:"
    ls -lh "$MODEL_DIR"/*.json "$MODEL_DIR"/*.txt 2>/dev/null || true
    echo ""
    echo "🚀 To use with ToroidalDB:"
    echo "   cargo build --features embeddings"
    echo "   cargo run --features embeddings"
else
    echo "❌ Model download failed!"
    exit 1
fi
