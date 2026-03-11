#!/bin/bash
set -euo pipefail

# HFT Deployment Script
# Usage: ./scripts/deploy.sh <target> [--tag <image_tag>] [--skip-pull]
#
# Targets:
#   binance    - Vultr server (Binance PAPI, 9 symbols)
#   binance2   - AWS Tokyo server (Binance PAPI, 15 expansion symbols)
#   bybit      - Hetzner server (Bybit)
#
# Examples:
#   ./scripts/deploy.sh binance                  # Deploy latest to Vultr
#   ./scripts/deploy.sh binance2                 # Deploy latest to AWS
#   ./scripts/deploy.sh bybit --tag v1.2.0       # Deploy specific tag to Hetzner
#   ./scripts/deploy.sh binance --skip-pull       # Restart with current image (config-only change)

# ─── Server Config ────────────────────────────────────────────────────────────

declare -A SSH_HOST=(
    [binance]="Vultur_mmtest0"
    [binance2]="AWS_LEE_ACCOUNT"
    [bybit]="Hetzner_bybit"
)
declare -A COMPOSE_FILE=(
    [binance]="docker-compose.binance.yml"
    [binance2]="docker-compose.binance.yml"
    [bybit]="docker-compose.mainnet.yml"
)
declare -A ENV_FILE=(
    [binance]=".env.binance"
    [binance2]=".env.bn2"
    [bybit]=".env.mainnet"
)
declare -A CONFIG_REPO_PATH=(
    [binance]="hft-configs"
    [binance2]="hft-configs"
    [bybit]="../../hft-configs"
)

DEPLOY_DIR="~/hftbacktest/deploy/docker"
REGISTRY="ghcr.io/bookdisco/hftbacktest"
IMAGE_TAG="latest"
SKIP_PULL=false

# ─── Parse Args ───────────────────────────────────────────────────────────────

TARGET="${1:-}"
shift || true

while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag)    IMAGE_TAG="$2"; shift 2 ;;
        --skip-pull) SKIP_PULL=true; shift ;;
        *)        echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [[ -z "$TARGET" ]] || [[ -z "${SSH_HOST[$TARGET]+x}" ]]; then
    echo "Usage: $0 <binance|binance2|bybit> [--tag <tag>] [--skip-pull]"
    exit 1
fi

HOST="${SSH_HOST[$TARGET]}"
COMPOSE="${COMPOSE_FILE[$TARGET]}"
ENVFILE="${ENV_FILE[$TARGET]}"
CONFIG_PATH="${CONFIG_REPO_PATH[$TARGET]}"

echo "═══════════════════════════════════════════"
echo " Deploying to: $TARGET ($HOST)"
echo " Compose:      $COMPOSE"
echo " Image tag:    $IMAGE_TAG"
echo " Skip pull:    $SKIP_PULL"
echo "═══════════════════════════════════════════"

# ─── Helper ───────────────────────────────────────────────────────────────────

remote() {
    ssh "$HOST" "cd $DEPLOY_DIR && $*"
}

# ─── Step 1: Update configs on server ─────────────────────────────────────────

echo ""
echo "▶ Step 1: Pulling latest configs..."
remote "cd $CONFIG_PATH && git pull --ff-only origin main"

# ─── Step 2: Pull Docker image ────────────────────────────────────────────────

if [[ "$SKIP_PULL" == false ]]; then
    echo ""
    echo "▶ Step 2: Pulling Docker image ${REGISTRY}:${IMAGE_TAG}..."
    remote "docker pull ${REGISTRY}:${IMAGE_TAG}"
else
    echo ""
    echo "▶ Step 2: Skipped (--skip-pull)"
fi

# ─── Step 3: Update deploy repo on server ─────────────────────────────────────

echo ""
echo "▶ Step 3: Pulling latest deploy files..."
remote "cd ../.. && git pull --ff-only origin master"
remote "chmod +x entrypoint.sh"

# ─── Step 4: Restart hft service ──────────────────────────────────────────────

echo ""
echo "▶ Step 4: Restarting hft service..."
remote "docker compose -f $COMPOSE --env-file $ENVFILE up -d hft"

# ─── Step 5: Verify ──────────────────────────────────────────────────────────

echo ""
echo "▶ Step 5: Waiting for startup..."
sleep 8

echo ""
echo "▶ Checking status..."
remote "docker compose -f $COMPOSE --env-file $ENVFILE exec hft hftctl --socket /tmp/hft-supervisor.sock status" || true

echo ""
echo "▶ Checking connector log (last 10 lines)..."
remote "docker compose -f $COMPOSE --env-file $ENVFILE exec hft tail -10 /var/lib/hft/logs/infra/connector_bf.log 2>/dev/null || \
        docker compose -f $COMPOSE --env-file $ENVFILE exec hft tail -10 /var/lib/hft/logs/infra/connector.log 2>/dev/null" || true

echo ""
echo "═══════════════════════════════════════════"
echo " Deploy complete: $TARGET ($HOST)"
echo "═══════════════════════════════════════════"
