#!/bin/bash
# Start OBI MM Strategy for Testnet
#
# This script runs the OBI MM (Order Book Imbalance Market Making) strategy.
# The connector must be running before starting this strategy.
#
# Usage: ./start_obi_mm.sh [options]
#
# Options:
#   --connector, -n    Connector name (default: bf)
#   --symbol, -s       Trading symbol (default: btcusdt)
#   --dry-run          Run without placing orders
#   --config, -c       Path to config file (optional)
#
# Example:
#   ./start_obi_mm.sh --connector bf --symbol btcusdt
#   ./start_obi_mm.sh --dry-run  # Test without placing orders

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Default values
CONNECTOR_NAME="bf"
SYMBOL="btcusdt"
DRY_RUN=""
CONFIG_FILE=""
EXTRA_ARGS=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--connector)
            CONNECTOR_NAME="$2"
            shift 2
            ;;
        -s|--symbol)
            SYMBOL="$2"
            shift 2
            ;;
        -c|--config)
            CONFIG_FILE="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN="--dry-run"
            shift
            ;;
        *)
            EXTRA_ARGS="$EXTRA_ARGS $1"
            shift
            ;;
    esac
done

echo "========================================"
echo " OBI MM Strategy (Testnet)"
echo "========================================"
echo "Connector: $CONNECTOR_NAME"
echo "Symbol: $SYMBOL"
if [ -n "$DRY_RUN" ]; then
    echo "Mode: DRY RUN (no actual orders)"
else
    echo "Mode: LIVE"
fi
if [ -n "$CONFIG_FILE" ]; then
    echo "Config: $CONFIG_FILE"
fi
echo "========================================"
echo ""

# Build config args
CONFIG_ARGS=""
if [ -n "$CONFIG_FILE" ]; then
    if [ ! -f "$CONFIG_FILE" ]; then
        echo "ERROR: Config file not found: $CONFIG_FILE"
        exit 1
    fi
    CONFIG_ARGS="--config $CONFIG_FILE"
fi

echo "Starting OBI MM strategy..."
echo "Press Ctrl+C to stop"
echo ""

cd "$ROOT_DIR"
RUST_LOG=info cargo run --release -p obi-mm --bin obi_mm_live --features live -- \
    --connector "$CONNECTOR_NAME" \
    --symbol "$SYMBOL" \
    $DRY_RUN \
    $CONFIG_ARGS \
    $EXTRA_ARGS
