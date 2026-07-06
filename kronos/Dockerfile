# ═══════════════════════════════════════════════════════════════════════════
# Kronos — AI Forecasting Service (Amazon Chronos-Bolt)
# FastAPI server providing time-series price predictions.
# ═══════════════════════════════════════════════════════════════════════════

FROM python:3.11-slim-bookworm
WORKDIR /app

# Install build dependencies for torch/numpy
RUN apt-get update && apt-get install -y \
    build-essential curl \
    && rm -rf /var/lib/apt/lists/*

# Copy and install Python deps
COPY requirements.txt .
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY main.py model.py tool.py download.py ./

EXPOSE 8000

HEALTHCHECK --interval=15s --timeout=5s --retries=10 \
    CMD curl -sf http://localhost:8000/health

CMD ["python", "main.py"]
