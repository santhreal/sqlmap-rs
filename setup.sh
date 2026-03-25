#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────
# sqlmap-rs: Automated environment setup
#
# Creates a conda environment with Python 3 and sqlmap,
# then verifies sqlmapapi is functional.
#
# Usage:
#   ./setup.sh              # Uses default env name "sqlmap-env"
#   ./setup.sh my-env       # Custom env name
#   source ./setup.sh       # Also activates the env in current shell
# ─────────────────────────────────────────────────────────
set -euo pipefail

ENV_NAME="${1:-sqlmap-env}"
SQLMAP_VERSION="${SQLMAP_VERSION:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[sqlmap-rs]${NC} $*"; }
ok()    { echo -e "${GREEN}[  OK  ]${NC} $*"; }
warn()  { echo -e "${YELLOW}[ WARN ]${NC} $*"; }
fail()  { echo -e "${RED}[FAILED]${NC} $*"; exit 1; }

# ── Detect conda ──────────────────────────────────────────
detect_conda() {
    if command -v conda &>/dev/null; then
        ok "conda found: $(conda --version)"
        return 0
    fi

    if command -v mamba &>/dev/null; then
        ok "mamba found (conda-compatible)"
        # alias for the rest of the script
        conda() { mamba "$@"; }
        export -f conda
        return 0
    fi

    # Check common install locations
    for path in ~/miniconda3/bin/conda ~/anaconda3/bin/conda /opt/conda/bin/conda; do
        if [ -x "$path" ]; then
            eval "$("$path" shell.bash hook)"
            ok "conda found at $path"
            return 0
        fi
    done

    return 1
}

# ── Install conda if missing ─────────────────────────────
install_conda() {
    info "conda not found. Installing Miniconda..."

    local installer="/tmp/miniconda_installer.sh"
    local arch
    arch="$(uname -m)"
    local os
    os="$(uname -s)"

    local url="https://repo.anaconda.com/miniconda/Miniconda3-latest-${os}-${arch}.sh"

    if command -v curl &>/dev/null; then
        curl -fsSL "$url" -o "$installer"
    elif command -v wget &>/dev/null; then
        wget -q "$url" -O "$installer"
    else
        fail "Neither curl nor wget found. Please install one and retry."
    fi

    bash "$installer" -b -p "$HOME/miniconda3"
    rm -f "$installer"

    eval "$("$HOME/miniconda3/bin/conda" shell.bash hook)"
    ok "Miniconda installed at ~/miniconda3"
}

# ── Create environment ────────────────────────────────────
create_env() {
    if conda env list 2>/dev/null | grep -q "^${ENV_NAME} "; then
        info "Environment '${ENV_NAME}' already exists"
        conda activate "${ENV_NAME}" 2>/dev/null || true
        return
    fi

    info "Creating conda environment '${ENV_NAME}' with Python 3.11..."
    conda create -n "${ENV_NAME}" python=3.11 -y -q

    conda activate "${ENV_NAME}" 2>/dev/null || true
    ok "Environment '${ENV_NAME}' created and activated"
}

# ── Install sqlmap ────────────────────────────────────────
install_sqlmap() {
    if command -v sqlmapapi &>/dev/null; then
        ok "sqlmapapi already available: $(which sqlmapapi)"
        return
    fi

    info "Installing sqlmap via pip..."
    if [ -n "$SQLMAP_VERSION" ]; then
        pip install -q "sqlmap==${SQLMAP_VERSION}"
    else
        pip install -q sqlmap
    fi

    # Verify the install
    if command -v sqlmapapi &>/dev/null; then
        ok "sqlmapapi installed: $(which sqlmapapi)"
    elif command -v sqlmap &>/dev/null; then
        # Some installs put sqlmapapi in a different location
        local sqlmap_dir
        sqlmap_dir="$(python3 -c 'import sqlmap; import os; print(os.path.dirname(sqlmap.__file__))' 2>/dev/null || true)"
        if [ -n "$sqlmap_dir" ] && [ -f "${sqlmap_dir}/sqlmapapi.py" ]; then
            warn "sqlmapapi not in PATH but found at: ${sqlmap_dir}/sqlmapapi.py"
            info "You can use binary_path option: SqlmapEngine::new_with_binary(\"python3 ${sqlmap_dir}/sqlmapapi.py\")"
        else
            fail "sqlmap installed but sqlmapapi not found. Try: pip install sqlmap"
        fi
    else
        fail "Failed to install sqlmap. Check your network and pip configuration."
    fi
}

# ── Verify everything works ───────────────────────────────
verify() {
    info "Running verification..."

    # Check Python
    local python_ver
    python_ver="$(python3 --version 2>&1)"
    ok "Python: ${python_ver}"

    # Check sqlmap import
    if python3 -c "import sqlmap; print(f'sqlmap {sqlmap.__version__}')" 2>/dev/null; then
        ok "sqlmap module imports correctly"
    else
        warn "sqlmap module import failed (may still work via CLI)"
    fi

    # Check sqlmapapi
    if sqlmapapi -h &>/dev/null; then
        ok "sqlmapapi responds to --help"
    else
        warn "sqlmapapi --help did not succeed"
    fi

    # Quick REST API smoke test
    info "Testing REST API boot (5 second timeout)..."
    local port=18775
    sqlmapapi -s -H 127.0.0.1 -p ${port} &>/dev/null &
    local pid=$!

    sleep 2
    if kill -0 "$pid" 2>/dev/null; then
        if curl -sf "http://127.0.0.1:${port}/task/new" &>/dev/null; then
            ok "sqlmapapi REST API is functional on port ${port}"
        else
            warn "sqlmapapi started but REST API not responding"
        fi
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    else
        warn "sqlmapapi process exited early"
    fi

    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  Setup complete!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════${NC}"
    echo ""
    echo "  Environment: ${ENV_NAME}"
    echo "  Activate:    conda activate ${ENV_NAME}"
    echo ""
    echo "  In your Rust code:"
    echo "    let engine = SqlmapEngine::new(8775, true, None).await?;"
    echo ""
}

# ── Main ──────────────────────────────────────────────────
main() {
    echo ""
    echo -e "${CYAN}sqlmap-rs Environment Setup${NC}"
    echo -e "${CYAN}══════════════════════════════════════${NC}"
    echo ""

    if ! detect_conda; then
        install_conda
    fi

    create_env
    install_sqlmap
    verify
}

main
