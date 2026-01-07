#!/bin/bash
# Manual API testing script for hft-server
#
# Usage:
#   ./scripts/test_api.sh [server_url]
#
# Default: http://localhost:3000

set -e

SERVER="${1:-http://localhost:3000}"
TOKEN=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=================================================="
echo "HFT Server API Test Script"
echo "Server: $SERVER"
echo "=================================================="

# Helper function
test_endpoint() {
    local method=$1
    local endpoint=$2
    local data=$3
    local expected_status=$4
    local description=$5

    echo -n "Testing: $description... "

    if [ -n "$data" ]; then
        response=$(curl -s -w "\n%{http_code}" -X "$method" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer $TOKEN" \
            -d "$data" \
            "$SERVER$endpoint" 2>/dev/null)
    else
        response=$(curl -s -w "\n%{http_code}" -X "$method" \
            -H "Authorization: Bearer $TOKEN" \
            "$SERVER$endpoint" 2>/dev/null)
    fi

    status_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | sed '$d')

    if [ "$status_code" == "$expected_status" ]; then
        echo -e "${GREEN}PASS${NC} (HTTP $status_code)"
        return 0
    else
        echo -e "${RED}FAIL${NC} (Expected $expected_status, got $status_code)"
        echo "Response: $body"
        return 1
    fi
}

# Test health check
echo ""
echo "=== Health Check ==="
test_endpoint "GET" "/api/health" "" "200" "Health check"

# Test without auth
echo ""
echo "=== Auth Tests (without token) ==="
test_endpoint "GET" "/api/auth/me" "" "401" "Get current user without token"

# Test login (if database is configured)
echo ""
echo "=== Login Test ==="
echo -n "Testing: Login with admin... "
login_response=$(curl -s -w "\n%{http_code}" -X POST \
    -H "Content-Type: application/json" \
    -d '{"username": "admin", "password": "admin"}' \
    "$SERVER/api/auth/login" 2>/dev/null)

login_status=$(echo "$login_response" | tail -n1)
login_body=$(echo "$login_response" | sed '$d')

if [ "$login_status" == "200" ]; then
    TOKEN=$(echo "$login_body" | jq -r '.token')
    echo -e "${GREEN}PASS${NC} - Got token"
else
    echo -e "${YELLOW}SKIP${NC} - Database not configured (HTTP $login_status)"
fi

# Test with auth (if we got a token)
if [ -n "$TOKEN" ] && [ "$TOKEN" != "null" ]; then
    echo ""
    echo "=== Auth Tests (with token) ==="
    test_endpoint "GET" "/api/auth/me" "" "200" "Get current user with token"
fi

# Test supervisor-dependent endpoints
echo ""
echo "=== Supervisor Tests ==="
echo -n "Testing: Get cluster status... "
status_response=$(curl -s -w "\n%{http_code}" -X GET \
    -H "Authorization: Bearer $TOKEN" \
    "$SERVER/api/status" 2>/dev/null)

status_code=$(echo "$status_response" | tail -n1)

if [ "$status_code" == "200" ]; then
    echo -e "${GREEN}PASS${NC} - Supervisor connected"
    SUPERVISOR_AVAILABLE=true
elif [ "$status_code" == "503" ]; then
    echo -e "${YELLOW}SKIP${NC} - Supervisor not available"
    SUPERVISOR_AVAILABLE=false
else
    echo -e "${RED}FAIL${NC} (HTTP $status_code)"
    SUPERVISOR_AVAILABLE=false
fi

if [ "$SUPERVISOR_AVAILABLE" == "true" ]; then
    test_endpoint "GET" "/api/bots" "" "200" "List bots"
fi

# Test config endpoints
echo ""
echo "=== Config Tests ==="
echo -n "Testing: Get cluster config... "
config_response=$(curl -s -w "\n%{http_code}" -X GET \
    -H "Authorization: Bearer $TOKEN" \
    "$SERVER/api/config/cluster" 2>/dev/null)

config_status=$(echo "$config_response" | tail -n1)

if [ "$config_status" == "200" ]; then
    echo -e "${GREEN}PASS${NC}"
elif [ "$config_status" == "404" ]; then
    echo -e "${YELLOW}SKIP${NC} - Config file not found"
else
    echo -e "${RED}FAIL${NC} (HTTP $config_status)"
fi

# Test invalid TOML
test_endpoint "PUT" "/api/config/cluster" '{"content": "invalid [ toml"}' "400" "Reject invalid TOML"

# Test data endpoints
echo ""
echo "=== Data Tests ==="
test_endpoint "GET" "/api/data/files" "" "200" "List data files"
test_endpoint "GET" "/api/data/stats" "" "200" "Get data stats"
test_endpoint "GET" "/api/data/warmup/BTCUSDT" "" "200" "Get warmup status"

# Test log endpoints
echo ""
echo "=== Log Tests ==="
test_endpoint "GET" "/api/logs/files" "" "200" "List log files"
test_endpoint "GET" "/api/logs/nonexistent" "" "404" "Get nonexistent log"

# Summary
echo ""
echo "=================================================="
echo "Test completed!"
echo "=================================================="
