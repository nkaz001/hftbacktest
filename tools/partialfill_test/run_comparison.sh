#!/bin/bash
# Run PartialFill vs NoPartialFill comparison tests
set -euo pipefail

BINARY="../../target/release/examples/partialfill_test"
RESULTS_DIR="../../results/partialfill_comparison"
DATA_BASE="/mnt/data/samba/users/Data/tardis/datasets/binance-futures/hft_npz"

mkdir -p "$RESULTS_DIR"

# Test configurations: symbol, tick_size, lot_size, base_qty, max_days
declare -A SYMBOLS
SYMBOLS[BTCUSDT]="0.1 0.001 0.001 30"
SYMBOLS[ETHUSDT]="0.01 0.001 0.01 30"
SYMBOLS[1000SHIBUSDT]="0.000001 1.0 100.0 30"

EXCHANGE_KINDS=("nopartial" "partial")
QTY_MULTIPLIERS=("1" "4" "16")

echo "=== PartialFill vs NoPartialFill Comparison ==="
echo "Start: $(date)"
echo ""

for SYMBOL in "${!SYMBOLS[@]}"; do
    read -r TICK LOT BASE_QTY MAX_DAYS <<< "${SYMBOLS[$SYMBOL]}"
    DATA_DIR="${DATA_BASE}/${SYMBOL}"

    if [ ! -d "$DATA_DIR" ]; then
        echo "SKIP: $SYMBOL — data dir not found: $DATA_DIR"
        continue
    fi

    for EXCH in "${EXCHANGE_KINDS[@]}"; do
        for QTY_MULT in "${QTY_MULTIPLIERS[@]}"; do
            RUN_NAME="${SYMBOL}_${EXCH}_${QTY_MULT}x"
            OUTPUT_CSV="${RESULTS_DIR}/${RUN_NAME}_0.csv"

            if [ -f "$OUTPUT_CSV" ]; then
                echo "SKIP: $RUN_NAME (already exists)"
                continue
            fi

            echo -n "RUN: $RUN_NAME ... "
            START_SEC=$SECONDS

            $BINARY \
                --data-dir "$DATA_DIR" \
                --symbol "$SYMBOL" \
                --exchange-kind "$EXCH" \
                --qty-multiplier "$QTY_MULT" \
                --tick-size "$TICK" \
                --lot-size "$LOT" \
                --base-qty "$BASE_QTY" \
                --max-days "$MAX_DAYS" \
                --output "$RESULTS_DIR" \
                2>"${RESULTS_DIR}/${RUN_NAME}.log"

            ELAPSED=$((SECONDS - START_SEC))
            echo "done (${ELAPSED}s)"
        done
    done
done

echo ""
echo "=== All runs complete ==="
echo "Results in: $RESULTS_DIR"
echo "End: $(date)"
