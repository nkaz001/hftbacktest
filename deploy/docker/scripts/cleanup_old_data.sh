#!/bin/bash
#
# Collector data cleanup script
# Runs via cron: 30 */6 * * * (every 6 hours)
#
# Usage: ./cleanup_old_data.sh
# Crontab: 30 */6 * * * /path/to/cleanup_old_data.sh >> /path/to/cleanup.log 2>&1

DATA_DIR=/root/hftbacktest/deploy/docker/data
RETENTION_DAYS=5      # Keep .gz files for 5 days
JSONL_MAX_AGE_HOURS=6 # Delete .jsonl files older than 6 hours (should be rotated daily)

echo "[$(date -Iseconds)] Starting data cleanup..."
echo "Disk before cleanup:"
df -h /

# Delete .gz files older than RETENTION_DAYS
echo "Cleaning .gz files older than ${RETENTION_DAYS} days..."
find "$DATA_DIR" -name "*.gz" -type f -mtime +$RETENTION_DAYS -print -delete

# Delete stale .jsonl files older than 6 hours (they should be rotated daily)
echo "Cleaning stale .jsonl files older than ${JSONL_MAX_AGE_HOURS} hours..."
find "$DATA_DIR" -name "*.jsonl" -type f -mmin +$((JSONL_MAX_AGE_HOURS * 60)) -print -delete

# Emergency cleanup: if disk > 85%, delete oldest .gz files
DISK_PCT=$(df / | tail -1 | awk '{print $5}' | tr -d '%')
if [ "$DISK_PCT" -gt 85 ]; then
    echo "WARNING: Disk at ${DISK_PCT}%, running emergency cleanup..."
    # Delete oldest .gz file for each symbol
    for sym in btcusdt ethusdt adausdt; do
        OLDEST=$(ls -t "$DATA_DIR"/${sym}_*.gz 2>/dev/null | tail -1)
        if [ -n "$OLDEST" ]; then
            echo "Emergency delete: $OLDEST"
            rm -f "$OLDEST"
        fi
    done
fi

echo "[$(date -Iseconds)] Cleanup complete."
echo "Disk after cleanup:"
df -h /
