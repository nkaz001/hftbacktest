#!/bin/bash
# Test script for warmup functionality
#
# This script:
# 1. Starts the connector (connects to Binance Futures Testnet)
# 2. Starts the collector in IPC mode (records data to .gz files)
# 3. Waits for enough data (15 minutes for warmup test)
# 4. Converts .gz to .npz
# 5. Tests the live strategy with warmup
#
# Prerequisites:
# - Build the binaries: cargo build --release -p connector -p collector -p obi-mm
# - Configure API keys in examples/testnet/config/binancefutures_testnet.toml

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

CONNECTOR_NAME="bf_test"
SYMBOL="btcusdt"
DATA_DIR="./warmup_test_data"
CONFIG_DIR="./examples/testnet/config"
STRATEGY_CONFIG="$CONFIG_DIR/obi_mm_warmup_test.toml"
CONNECTOR_CONFIG="$CONFIG_DIR/binancefutures_testnet.toml"

# Warmup timing (in seconds)
COLLECT_TIME=900  # 15 minutes - enough for window=60 (10 min warmup)

echo "========================================="
echo " HFT Backtest Warmup Test"
echo "========================================="
echo "Connector: $CONNECTOR_NAME"
echo "Symbol: $SYMBOL"
echo "Data dir: $DATA_DIR"
echo "Collection time: $COLLECT_TIME seconds"
echo ""

# Create data directory
mkdir -p "$DATA_DIR"

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "Cleaning up..."
    if [ ! -z "$CONNECTOR_PID" ]; then
        echo "Stopping connector (PID: $CONNECTOR_PID)"
        kill $CONNECTOR_PID 2>/dev/null || true
    fi
    if [ ! -z "$COLLECTOR_PID" ]; then
        echo "Stopping collector (PID: $COLLECTOR_PID)"
        kill $COLLECTOR_PID 2>/dev/null || true
    fi
    echo "Cleanup complete."
}
trap cleanup EXIT

# Step 1: Start connector
echo "Step 1: Starting connector..."
./target/release/connector \
    --name "$CONNECTOR_NAME" \
    binancefutures \
    "$CONNECTOR_CONFIG" &
CONNECTOR_PID=$!
echo "Connector started (PID: $CONNECTOR_PID)"
sleep 5  # Wait for connector to initialize

# Step 2: Start collector in IPC mode
echo ""
echo "Step 2: Starting collector in IPC mode..."
./target/release/collector "$DATA_DIR" --ipc "$CONNECTOR_NAME" "$SYMBOL" &
COLLECTOR_PID=$!
echo "Collector started (PID: $COLLECTOR_PID)"
sleep 2

# Step 3: Wait for data collection
echo ""
echo "Step 3: Collecting data for $COLLECT_TIME seconds..."
echo "   This will take ~15 minutes. Press Ctrl+C to stop early."
echo ""

# Show progress every 60 seconds
ELAPSED=0
while [ $ELAPSED -lt $COLLECT_TIME ]; do
    sleep 60
    ELAPSED=$((ELAPSED + 60))

    # Check if processes are still running
    if ! kill -0 $CONNECTOR_PID 2>/dev/null; then
        echo "ERROR: Connector died unexpectedly"
        exit 1
    fi
    if ! kill -0 $COLLECTOR_PID 2>/dev/null; then
        echo "ERROR: Collector died unexpectedly"
        exit 1
    fi

    # Show progress
    GZ_FILES=$(ls -la "$DATA_DIR"/*.gz 2>/dev/null | wc -l || echo "0")
    GZ_SIZE=$(du -sh "$DATA_DIR" 2>/dev/null | cut -f1 || echo "0")
    echo "   Progress: $ELAPSED/$COLLECT_TIME seconds | Files: $GZ_FILES | Size: $GZ_SIZE"
done

# Stop collector first (connector needs to keep running for live strategy)
echo ""
echo "Step 4: Stopping collector..."
kill $COLLECTOR_PID 2>/dev/null || true
COLLECTOR_PID=""
sleep 2

# Step 5: Convert .gz to .npz
echo ""
echo "Step 5: Converting .gz files to .npz..."
python3 -c "
import os
import sys
sys.path.insert(0, '$PROJECT_DIR/py-hftbacktest')

from hftbacktest.data.utils.binancefutures import convert

data_dir = '$DATA_DIR'
for filename in os.listdir(data_dir):
    if filename.endswith('.gz'):
        input_path = os.path.join(data_dir, filename)
        output_path = input_path.replace('.gz', '.npz')
        print(f'Converting {filename}...')
        try:
            convert(input_path, output_path)
            print(f'  -> {os.path.basename(output_path)}')
        except Exception as e:
            print(f'  ERROR: {e}')
"

# List generated files
echo ""
echo "Generated files:"
ls -la "$DATA_DIR"/*.npz 2>/dev/null || echo "No .npz files generated"

# Step 6: Test live strategy with warmup
echo ""
echo "Step 6: Testing live strategy with warmup..."
echo "   Config: $STRATEGY_CONFIG"
echo "   Warmup data: $DATA_DIR"
echo ""

# Run strategy for 60 seconds to verify warmup
timeout 60 ./target/release/obi_mm_live \
    --connector "$CONNECTOR_NAME" \
    --symbol "$SYMBOL" \
    --config "$STRATEGY_CONFIG" \
    --warmup-data-dir "$DATA_DIR" \
    --warmup-max-age 3600 \
    --warmup-min-duration 600 \
    --dry-run \
    2>&1 || true

echo ""
echo "========================================="
echo " Test Complete"
echo "========================================="
echo ""
echo "To run the full live strategy:"
echo ""
echo "  ./target/release/obi_mm_live \\"
echo "    --connector $CONNECTOR_NAME \\"
echo "    --symbol $SYMBOL \\"
echo "    --config $STRATEGY_CONFIG \\"
echo "    --warmup-data-dir $DATA_DIR"
