# RAT Agent — Deployment Guide

## Production Deployment

### 1. Build Release Binary

```bash
cargo build --release
```

### 2. Configure Environment

```bash
# Create .env file
cat > .env << EOF
MEMORY_API_URL=http://localhost:3111
BINANCE_API_KEY=your_key
BINANCE_API_SECRET=your_secret
TRADING_MODE=paper
EOF
```

### 3. Start Services

```bash
# Start memory service
cd memory && cargo run --release

# Start trading pipeline
./target/release/rat-pipeline --symbols BTC,ETH --mode paper
```

### 4. Docker Deployment (Optional)

```dockerfile
FROM rust:1.96 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/rat-pipeline /usr/local/bin/
CMD ["rat-pipeline", "--symbols", "BTC,ETH", "--mode", "paper"]
```

## Monitoring

```bash
# Check pipeline status
curl http://localhost:8080/health

# View logs
tail -f logs/rat.log

# Check memory usage
ps aux | grep rat-pipeline
```

## Scaling

- Add more symbols: `--symbols BTC,ETH,SOL,ADA,XRP`
- Run multiple instances for different strategies
- Use load balancer for high-frequency trading
