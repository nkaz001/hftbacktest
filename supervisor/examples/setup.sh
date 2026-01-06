#!/bin/bash
# HFT Supervisor Setup Script
# Run as root or with sudo

set -e

echo "=== HFT Supervisor Setup ==="

# Create user if not exists
if ! id -u hft >/dev/null 2>&1; then
    echo "Creating hft user..."
    useradd --system --no-create-home --shell /sbin/nologin hft
fi

# Create directories
echo "Creating directories..."
mkdir -p /opt/hft/bin
mkdir -p /etc/hft/connector
mkdir -p /etc/hft/strategies
mkdir -p /var/lib/hft/data/raw
mkdir -p /var/log/hft/bots
mkdir -p /run/hft

# Set ownership
chown -R hft:hft /var/lib/hft
chown -R hft:hft /var/log/hft
chown -R hft:hft /run/hft
chown root:hft /etc/hft
chmod 750 /etc/hft

# Copy binaries (adjust source path as needed)
BINARY_DIR="${1:-./target/release}"
if [ -d "$BINARY_DIR" ]; then
    echo "Installing binaries from $BINARY_DIR..."
    cp -f "$BINARY_DIR/connector" /opt/hft/bin/ 2>/dev/null || true
    cp -f "$BINARY_DIR/collector" /opt/hft/bin/ 2>/dev/null || true
    cp -f "$BINARY_DIR/hft-supervisor" /opt/hft/bin/ 2>/dev/null || true
    cp -f "$BINARY_DIR/hftctl" /opt/hft/bin/ 2>/dev/null || true
    cp -f "$BINARY_DIR/obi_mm" /opt/hft/bin/ 2>/dev/null || true
    chmod +x /opt/hft/bin/*
else
    echo "Binary directory not found: $BINARY_DIR"
    echo "Please build first: cargo build --release"
fi

# Install systemd services
echo "Installing systemd services..."
cp examples/systemd/*.service /etc/systemd/system/
systemctl daemon-reload

# Install hftctl to PATH
if [ -f /opt/hft/bin/hftctl ]; then
    ln -sf /opt/hft/bin/hftctl /usr/local/bin/hftctl
fi

# Copy example config if not exists
if [ ! -f /etc/hft/cluster.toml ]; then
    echo "Installing example configuration..."
    cp examples/cluster.toml /etc/hft/
    chown root:hft /etc/hft/cluster.toml
    chmod 640 /etc/hft/cluster.toml
    echo "IMPORTANT: Edit /etc/hft/cluster.toml and /etc/hft/connector/binancefutures.toml"
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo "1. Edit /etc/hft/cluster.toml with your configuration"
echo "2. Add your API keys to /etc/hft/connector/binancefutures.toml"
echo "3. Add strategy configs to /etc/hft/strategies/"
echo "4. Start services:"
echo "   systemctl start hft-connector"
echo "   systemctl start hft-collector"
echo "   systemctl start hft-supervisor"
echo ""
echo "5. Check status:"
echo "   hftctl status"
echo ""
echo "6. Enable on boot:"
echo "   systemctl enable hft-connector hft-collector hft-supervisor"
