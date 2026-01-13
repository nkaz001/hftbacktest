#!/bin/bash
#
# Batch convert Tardis data to NPZ format
# Usage:
#   ./batch_convert.sh                    # Process all symbols and dates
#   ./batch_convert.sh BTCUSDT            # Process specific symbol
#   ./batch_convert.sh BTCUSDT 2025-06-30 # Process specific symbol and date
#   ./batch_convert.sh --list             # List available symbols
#   ./batch_convert.sh --dry-run          # Show what would be processed without running
#

set -e

BASE_PATH="/mnt/data/samba/users/Data/tardis/datasets/binance-futures"
TRADES_DIR="$BASE_PATH/trades"
DEPTH_DIR="$BASE_PATH/incremental_book_L2"
OUTPUT_DIR="$BASE_PATH/hft_npz"
CONVERTER="./target/release/examples/tardis_convert2"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parallel jobs (adjust based on your CPU)
PARALLEL_JOBS=${PARALLEL_JOBS:-4}

# Log file
LOG_DIR="$OUTPUT_DIR/logs"
LOG_FILE="$LOG_DIR/batch_convert_$(date +%Y%m%d_%H%M%S).log"

# Counters
TOTAL_PROCESSED=0
TOTAL_SKIPPED=0
TOTAL_FAILED=0

log() {
    echo -e "$1" | tee -a "$LOG_FILE"
}

# Check if converter exists
check_converter() {
    if [ ! -f "$CONVERTER" ]; then
        echo -e "${RED}Error: Converter not found at $CONVERTER${NC}"
        echo "Please build it first:"
        echo "  cargo build --release -p hftbacktest --example tardis_convert2 --no-default-features --features backtest"
        exit 1
    fi
}

# List all symbols that have both trades and depth data
list_symbols() {
    comm -12 \
        <(ls "$TRADES_DIR" 2>/dev/null | sort) \
        <(ls "$DEPTH_DIR" 2>/dev/null | sort)
}

# Get dates that have both trades and depth data for a symbol
get_valid_dates() {
    local symbol=$1
    local trades_dates=$(ls "$TRADES_DIR/$symbol"/*.csv.gz 2>/dev/null | xargs -n1 basename | sed 's/.csv.gz//' | sort)
    local depth_dates=$(ls "$DEPTH_DIR/$symbol"/*.csv.gz 2>/dev/null | xargs -n1 basename | sed 's/.csv.gz//' | sort)

    comm -12 <(echo "$trades_dates") <(echo "$depth_dates")
}

# Check if output already exists
output_exists() {
    local symbol=$1
    local date=$2
    local date_formatted=$(echo "$date" | tr -d '-')
    local output_file="$OUTPUT_DIR/$symbol/${symbol}_${date_formatted}.npz"
    [ -f "$output_file" ]
}

# Convert a single day
convert_day() {
    local symbol=$1
    local date=$2

    # Check if already converted
    if output_exists "$symbol" "$date"; then
        log "${YELLOW}[SKIP]${NC} $symbol $date - already converted"
        ((TOTAL_SKIPPED++)) || true
        return 0
    fi

    log "${BLUE}[CONVERT]${NC} $symbol $date"

    if $CONVERTER --symbol "$symbol" --date "$date" >> "$LOG_FILE" 2>&1; then
        log "${GREEN}[OK]${NC} $symbol $date"
        ((TOTAL_PROCESSED++)) || true
    else
        log "${RED}[FAIL]${NC} $symbol $date"
        ((TOTAL_FAILED++)) || true
    fi
}

# Convert all dates for a symbol
convert_symbol() {
    local symbol=$1
    local dates=$(get_valid_dates "$symbol")
    local count=$(echo "$dates" | wc -w)

    if [ -z "$dates" ]; then
        log "${YELLOW}[SKIP]${NC} $symbol - no valid dates found"
        return 0
    fi

    log "${BLUE}========================================${NC}"
    log "${BLUE}Processing $symbol ($count dates)${NC}"
    log "${BLUE}========================================${NC}"

    for date in $dates; do
        convert_day "$symbol" "$date"
    done
}

# Print summary
print_summary() {
    log ""
    log "${BLUE}========================================${NC}"
    log "${BLUE}Batch Conversion Summary${NC}"
    log "${BLUE}========================================${NC}"
    log "  Processed: ${GREEN}$TOTAL_PROCESSED${NC}"
    log "  Skipped:   ${YELLOW}$TOTAL_SKIPPED${NC}"
    log "  Failed:    ${RED}$TOTAL_FAILED${NC}"
    log "  Log file:  $LOG_FILE"
    log "${BLUE}========================================${NC}"
}

# Dry run - show what would be processed
dry_run() {
    local symbols=$(list_symbols)
    local total_days=0
    local total_to_convert=0

    echo -e "${BLUE}Dry Run - Analyzing available data...${NC}"
    echo ""

    for symbol in $symbols; do
        local dates=$(get_valid_dates "$symbol")
        local count=$(echo "$dates" | wc -w)
        local to_convert=0

        for date in $dates; do
            if ! output_exists "$symbol" "$date"; then
                ((to_convert++)) || true
            fi
        done

        if [ "$count" -gt 0 ]; then
            echo -e "$symbol: $count dates available, ${GREEN}$to_convert${NC} to convert"
            ((total_days += count)) || true
            ((total_to_convert += to_convert)) || true
        fi
    done

    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "Total symbols: $(echo "$symbols" | wc -w)"
    echo -e "Total dates available: $total_days"
    echo -e "Total to convert: ${GREEN}$total_to_convert${NC}"
    echo -e "${BLUE}========================================${NC}"
}

# Main
main() {
    # Handle special flags
    case "${1:-}" in
        --list)
            echo "Available symbols with both trades and depth data:"
            list_symbols
            exit 0
            ;;
        --dry-run)
            dry_run
            exit 0
            ;;
        --help|-h)
            echo "Usage:"
            echo "  $0                         # Process all symbols and dates"
            echo "  $0 BTCUSDT                 # Process specific symbol"
            echo "  $0 BTCUSDT 2025-06-30      # Process specific symbol and date"
            echo "  $0 --list                  # List available symbols"
            echo "  $0 --dry-run               # Show what would be processed"
            echo ""
            echo "Environment variables:"
            echo "  PARALLEL_JOBS=4            # Number of parallel jobs (not implemented yet)"
            exit 0
            ;;
    esac

    check_converter

    # Create log directory
    mkdir -p "$LOG_DIR"

    log "${BLUE}========================================${NC}"
    log "${BLUE}Tardis Batch Converter${NC}"
    log "${BLUE}Started: $(date)${NC}"
    log "${BLUE}========================================${NC}"

    if [ -n "${1:-}" ] && [ -n "${2:-}" ]; then
        # Convert specific symbol and date
        convert_day "$1" "$2"
    elif [ -n "${1:-}" ]; then
        # Convert all dates for specific symbol
        convert_symbol "$1"
    else
        # Convert all symbols
        local symbols=$(list_symbols)
        local total_symbols=$(echo "$symbols" | wc -w)
        local current=0

        log "Found $total_symbols symbols to process"
        log ""

        for symbol in $symbols; do
            ((current++)) || true
            log "${BLUE}[$current/$total_symbols]${NC} Processing $symbol..."
            convert_symbol "$symbol"
        done
    fi

    log ""
    log "${BLUE}Finished: $(date)${NC}"
    print_summary
}

main "$@"
