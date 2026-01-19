#!/bin/bash
# Server Environment Setup Script
# Sets up zsh, oh-my-zsh, plugins, starship, and miniconda

set -e

echo "=========================================="
echo "  Server Environment Setup"
echo "=========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# ==========================================
# 1. Install dependencies
# ==========================================
log_info "Installing dependencies..."
apt-get update
apt-get install -y \
    zsh \
    git \
    curl \
    wget \
    vim \
    htop \
    tree \
    unzip \
    locales \
    ca-certificates

# Fix locale
log_info "Setting up locale..."
locale-gen en_US.UTF-8
update-locale LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

# ==========================================
# 2. Install Oh My Zsh
# ==========================================
log_info "Installing Oh My Zsh..."
if [ ! -d "$HOME/.oh-my-zsh" ]; then
    sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended
else
    log_warn "Oh My Zsh already installed, skipping..."
fi

# ==========================================
# 3. Install zsh plugins
# ==========================================
log_info "Installing zsh-autosuggestions..."
ZSH_CUSTOM="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"
if [ ! -d "$ZSH_CUSTOM/plugins/zsh-autosuggestions" ]; then
    git clone https://github.com/zsh-users/zsh-autosuggestions "$ZSH_CUSTOM/plugins/zsh-autosuggestions"
else
    log_warn "zsh-autosuggestions already installed, skipping..."
fi

log_info "Installing zsh-syntax-highlighting..."
if [ ! -d "$ZSH_CUSTOM/plugins/zsh-syntax-highlighting" ]; then
    git clone https://github.com/zsh-users/zsh-syntax-highlighting.git "$ZSH_CUSTOM/plugins/zsh-syntax-highlighting"
else
    log_warn "zsh-syntax-highlighting already installed, skipping..."
fi

# ==========================================
# 4. Install Starship prompt
# ==========================================
log_info "Installing Starship prompt..."
if ! command -v starship &> /dev/null; then
    curl -sS https://starship.rs/install.sh | sh -s -- -y
else
    log_warn "Starship already installed, skipping..."
fi

# ==========================================
# 5. Install Miniconda
# ==========================================
log_info "Installing Miniconda..."
MINICONDA_DIR="$HOME/miniconda3"
if [ ! -d "$MINICONDA_DIR" ]; then
    wget -q https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh -O /tmp/miniconda.sh
    bash /tmp/miniconda.sh -b -p "$MINICONDA_DIR"
    rm /tmp/miniconda.sh

    # Initialize conda for zsh
    "$MINICONDA_DIR/bin/conda" init zsh
else
    log_warn "Miniconda already installed, skipping..."
fi

# ==========================================
# 6. Configure .zshrc
# ==========================================
log_info "Configuring .zshrc..."
cat > "$HOME/.zshrc" << 'ZSHRC'
# Path to oh-my-zsh installation
export ZSH="$HOME/.oh-my-zsh"

# Theme (using starship instead)
ZSH_THEME=""

# Plugins
plugins=(
    git
    zsh-autosuggestions
    zsh-syntax-highlighting
    extract
    docker
    docker-compose
)

source $ZSH/oh-my-zsh.sh

# ==========================================
# User Configuration
# ==========================================

# Language
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

# Editor
export EDITOR='vim'

# History
HISTSIZE=10000
SAVEHIST=10000
setopt SHARE_HISTORY
setopt HIST_IGNORE_DUPS

# ==========================================
# Aliases
# ==========================================

# General
alias ll='ls -alF'
alias la='ls -A'
alias l='ls -CF'
alias ..='cd ..'
alias ...='cd ../..'

# Docker
alias dc='docker compose'
alias dps='docker ps'
alias dlog='docker logs -f'

# HFT specific
alias hft='cd ~/hftbacktest/deploy/docker'
alias hftctl='docker compose exec hft hftctl --socket /tmp/hft-supervisor.sock'
alias hftlogs='docker compose logs -f hft'
alias hftstatus='docker compose exec hft hftctl --socket /tmp/hft-supervisor.sock status'

# ==========================================
# Starship Prompt
# ==========================================
eval "$(starship init zsh)"

# ==========================================
# Conda Initialize
# ==========================================
# >>> conda initialize >>>
__conda_setup="$('$HOME/miniconda3/bin/conda' 'shell.zsh' 'hook' 2> /dev/null)"
if [ $? -eq 0 ]; then
    eval "$__conda_setup"
else
    if [ -f "$HOME/miniconda3/etc/profile.d/conda.sh" ]; then
        . "$HOME/miniconda3/etc/profile.d/conda.sh"
    else
        export PATH="$HOME/miniconda3/bin:$PATH"
    fi
fi
unset __conda_setup
# <<< conda initialize <<<
ZSHRC

# ==========================================
# 7. Configure Starship
# ==========================================
log_info "Configuring Starship..."
mkdir -p "$HOME/.config"
cat > "$HOME/.config/starship.toml" << 'STARSHIP'
# Starship Configuration

# Timeout for commands
command_timeout = 1000

# Format
format = """
$username\
$hostname\
$directory\
$git_branch\
$git_status\
$python\
$rust\
$docker_context\
$cmd_duration\
$line_break\
$character"""

[character]
success_symbol = "[➜](bold green)"
error_symbol = "[✗](bold red)"

[directory]
truncation_length = 3
truncate_to_repo = true

[git_branch]
symbol = " "
format = "[$symbol$branch]($style) "

[git_status]
format = '([$all_status$ahead_behind]($style) )'

[python]
symbol = " "
format = '[${symbol}${pyenv_prefix}(${version} )(\($virtualenv\) )]($style)'

[rust]
symbol = " "
format = "[$symbol($version )]($style)"

[docker_context]
symbol = " "
format = "[$symbol$context]($style) "

[cmd_duration]
min_time = 2000
format = "took [$duration](bold yellow) "

[hostname]
ssh_only = true
format = "[@$hostname](bold red) "
STARSHIP

# ==========================================
# 8. Set zsh as default shell
# ==========================================
log_info "Setting zsh as default shell..."
chsh -s $(which zsh) || log_warn "Could not change default shell, may need to run manually"

# ==========================================
# 9. Install Rust (for development)
# ==========================================
log_info "Installing Rust..."
if ! command -v rustc &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    log_warn "Rust already installed, skipping..."
fi

# ==========================================
# Done!
# ==========================================
echo ""
echo "=========================================="
echo -e "${GREEN}  Setup Complete!${NC}"
echo "=========================================="
echo ""
echo "Installed:"
echo "  - zsh + oh-my-zsh"
echo "  - Plugins: git, zsh-autosuggestions, zsh-syntax-highlighting, extract, docker"
echo "  - Starship prompt"
echo "  - Miniconda"
echo "  - Rust"
echo ""
echo "To apply changes, run:"
echo "  exec zsh"
echo ""
echo "HFT shortcuts available:"
echo "  hft       - cd to deploy/docker"
echo "  hftstatus - show bot status"
echo "  hftlogs   - follow hft logs"
echo ""
