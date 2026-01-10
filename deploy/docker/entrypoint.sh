#!/bin/bash
set -e

# HFT System Docker Entrypoint
# This script:
# 1. Substitutes environment variables in config files (as root)
# 2. Waits for ClickHouse to be ready
# 3. Drops privileges to hft user
# 4. Starts the supervisor which manages all other processes

echo "========================================"
echo " HFT System Starting"
echo "========================================"
echo "Config: ${CLUSTER_CONFIG:-/etc/hft/cluster.toml}"
echo "ClickHouse: ${CLICKHOUSE_URL:-http://clickhouse:8123}"

# Clean up iceoryx2 IPC state from previous runs
# This prevents "ServiceInCorruptedState" and "DoesNotSupportRequestedAmountOfPublishers" errors
echo "Cleaning iceoryx2 IPC state..."
rm -rf /dev/shm/iox2_* 2>/dev/null || true
rm -rf /tmp/iceoryx2/services/* 2>/dev/null || true
rm -rf /tmp/iceoryx2/nodes/* 2>/dev/null || true
mkdir -p /tmp/iceoryx2/services /tmp/iceoryx2/nodes
chown -R hft:hft /tmp/iceoryx2
echo "IPC state cleaned"

# Create temporary config directory for processed files
CONFIG_DIR="/tmp/hft-config"
mkdir -p "$CONFIG_DIR"
chown hft:hft "$CONFIG_DIR"

# Substitute environment variables in binancefutures.toml
if [ -f /etc/hft/binancefutures.toml ]; then
    echo "Processing binancefutures.toml..."
    sed -e "s|\${BINANCE_API_KEY}|${BINANCE_API_KEY:-}|g" \
        -e "s|\${BINANCE_SECRET}|${BINANCE_SECRET:-}|g" \
        /etc/hft/binancefutures.toml > "$CONFIG_DIR/binancefutures.toml"
    chown hft:hft "$CONFIG_DIR/binancefutures.toml"

    # Verify API key is set
    if [ -z "$BINANCE_API_KEY" ] || [ "$BINANCE_API_KEY" = "your_binance_api_key_here" ]; then
        echo "WARNING: BINANCE_API_KEY not set properly"
    fi
fi

# Substitute environment variables in bybit.toml
if [ -f /etc/hft/bybit.toml ]; then
    echo "Processing bybit.toml..."
    sed -e "s|\${BYBIT_API_KEY}|${BYBIT_API_KEY:-}|g" \
        -e "s|\${BYBIT_SECRET}|${BYBIT_SECRET:-}|g" \
        /etc/hft/bybit.toml > "$CONFIG_DIR/bybit.toml"
    chown hft:hft "$CONFIG_DIR/bybit.toml"

    # Verify API key is set
    if [ -z "$BYBIT_API_KEY" ] || [ "$BYBIT_API_KEY" = "your_bybit_api_key_here" ]; then
        echo "WARNING: BYBIT_API_KEY not set properly"
    fi
fi

# Copy strategy configs to temp directory
if [ -d /etc/hft/strategies ]; then
    echo "Processing strategy configs..."
    mkdir -p "$CONFIG_DIR/strategies"
    cp -r /etc/hft/strategies/* "$CONFIG_DIR/strategies/"
    chown -R hft:hft "$CONFIG_DIR/strategies"
fi

# Copy cluster.toml and update config paths
if [ -f /etc/hft/cluster.toml ]; then
    echo "Processing cluster.toml..."
    sed -e "s|/etc/hft/binancefutures.toml|$CONFIG_DIR/binancefutures.toml|g" \
        -e "s|/etc/hft/bybit.toml|$CONFIG_DIR/bybit.toml|g" \
        -e "s|/etc/hft/strategies|$CONFIG_DIR/strategies|g" \
        /etc/hft/cluster.toml > "$CONFIG_DIR/cluster.toml"
    chown hft:hft "$CONFIG_DIR/cluster.toml"
fi

# Ensure data directories are owned by hft user
chown -R hft:hft /var/lib/hft

# Wait for ClickHouse to be ready (with timeout)
CLICKHOUSE_HOST="${CLICKHOUSE_URL:-http://clickhouse:8123}"
echo "Waiting for ClickHouse at $CLICKHOUSE_HOST..."

MAX_RETRIES=30
RETRY_COUNT=0
until curl -sf "$CLICKHOUSE_HOST/ping" > /dev/null 2>&1; do
    RETRY_COUNT=$((RETRY_COUNT + 1))
    if [ $RETRY_COUNT -ge $MAX_RETRIES ]; then
        echo "WARNING: ClickHouse not ready after $MAX_RETRIES attempts, continuing anyway..."
        break
    fi
    echo "Waiting for ClickHouse... ($RETRY_COUNT/$MAX_RETRIES)"
    sleep 2
done

if [ $RETRY_COUNT -lt $MAX_RETRIES ]; then
    echo "ClickHouse is ready!"
fi

# Start the HFT Server in background (for web dashboard) as hft user
echo "Starting HFT Server..."
gosu hft /usr/local/bin/hft-server \
    --config "$CONFIG_DIR/cluster.toml" \
    --socket /tmp/hft-supervisor.sock \
    --bind 0.0.0.0 \
    --port 3000 \
    --database-url "sqlite:/var/lib/hft/hft-server.db?mode=rwc" \
    ${JWT_SECRET:+--jwt-secret "$JWT_SECRET"} \
    ${NO_AUTH:+--no-auth} \
    > /var/lib/hft/logs/infra/server.log 2>&1 &

SERVER_PID=$!
echo "HFT Server started (PID: $SERVER_PID)"

# Give server a moment to start
sleep 2

# Start the supervisor (this is the main process) as hft user
echo "Starting HFT Supervisor..."
exec gosu hft /usr/local/bin/hft-supervisor --config "$CONFIG_DIR/cluster.toml" --bin-dir /usr/local/bin
