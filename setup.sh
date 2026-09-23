#!/usr/bin/env bash
#
# Open FPGA toolchain bootstrapper
#
# Installs/builds:
#   - Yosys
#   - Verilator
#   - Project Trellis / libtrellis
#   - nextpnr-ecp5
#
# Normal behavior:
#   - If all required commands already exist, just report their versions.
#   - If anything is missing, install prerequisites and build the missing tools.
#
# Force a rebuild/update:
#   FORCE_REBUILD=1 ./setup.sh
#
# Assumptions:
#   - Linux
#   - Debian/Ubuntu-style apt package manager
#   - sudo access
#
# Install prefix:
#   /usr/local
#
# Source directory:
#   $HOME/.local/src/fpga-toolchain
#

set -Eeuo pipefail

###############################################################################
# Configuration
###############################################################################

PREFIX="${PREFIX:-/usr/local}"
SRC_ROOT="${SRC_ROOT:-${HOME}/.local/src/fpga-toolchain}"
JOBS="${JOBS:-$(nproc)}"
FORCE_REBUILD="${FORCE_REBUILD:-0}"

YOSYS_REPO="https://github.com/YosysHQ/yosys.git"
VERILATOR_REPO="https://github.com/verilator/verilator.git"
TRELLIS_REPO="https://github.com/YosysHQ/prjtrellis.git"
NEXTPNR_REPO="https://github.com/YosysHQ/nextpnr.git"

###############################################################################
# Logging
###############################################################################

log()
{
    printf '\n\033[1;34m==>\033[0m %s\n' "$*"
}

warn()
{
    printf '\n\033[1;33mWARNING:\033[0m %s\n' "$*" >&2
}

die()
{
    printf '\n\033[1;31mERROR:\033[0m %s\n' "$*" >&2
    exit 1
}

###############################################################################
# Basic checks
###############################################################################

[[ "$(uname -s)" == "Linux" ]] ||
    die "This setup script currently supports Linux only."

command -v sudo >/dev/null 2>&1 ||
    die "sudo is required."

command -v apt-get >/dev/null 2>&1 ||
    die "apt-get is required. This script targets Debian/Ubuntu-style systems."

command -v git >/dev/null 2>&1 ||
    die "git is required to bootstrap the toolchain."

mkdir -p "$SRC_ROOT"

###############################################################################
# Version reporting
###############################################################################

report_command()
{
    local cmd="$1"

    if command -v "$cmd" >/dev/null 2>&1; then
        printf "  %-18s " "$cmd"

        case "$cmd" in
            yosys)
                yosys --version 2>&1 | head -n1
                ;;
            verilator)
                verilator --version 2>&1 | head -n1
                ;;
            nextpnr-ecp5)
                nextpnr-ecp5 --version 2>&1 | head -n1
                ;;
            ecppack)
                ecppack --version 2>&1 | head -n1
                ;;
            *)
                "$cmd" --version 2>&1 | head -n1
                ;;
        esac
    else
        printf "  %-18s MISSING\n" "$cmd"
        return 1
    fi
}

report_toolchain()
{
    log "Installed toolchain"

    local missing=0

    report_command yosys         || missing=1
    report_command verilator     || missing=1
    report_command nextpnr-ecp5  || missing=1
    report_command ecppack       || missing=1

    return "$missing"
}

###############################################################################
# Initial state
###############################################################################

log "Checking installed toolchain"

if [[ "$FORCE_REBUILD" != "1" ]] && report_toolchain; then
    log "All required commands are installed."
    log "Nothing to build."

    cat <<EOF

Toolchain is installed.

To force a rebuild from the current upstream git repositories:

    FORCE_REBUILD=1 $0

EOF

    exit 0
fi

if [[ "$FORCE_REBUILD" == "1" ]]; then
    warn "FORCE_REBUILD=1: rebuilding the toolchain from upstream master."
fi

###############################################################################
# Prerequisites
###############################################################################

install_prerequisites()
{
    log "Installing build prerequisites"

    sudo apt-get update

    sudo apt-get install -y \
        build-essential \
        git \
        ca-certificates \
        curl \
        wget \
        pkg-config \
        gawk \
        make \
        cmake \
        python3 \
        python3-dev \
        python3-pip \
        python3-venv \
        bison \
        flex \
        autoconf \
        libffi-dev \
        libfl-dev \
        libreadline-dev \
        libboost-all-dev \
        libeigen3-dev \
        libtclap-dev \
        tcl-dev \
        zlib1g-dev \
        liblz4-dev \
        help2man \
        perl \
        perl-doc \
        ccache \
        graphviz \
        xdot \
        lld \
        clang
}

install_prerequisites

###############################################################################
# Repository helpers
###############################################################################

clone_or_update()
{
    local name="$1"
    local repo="$2"
    local dir="${SRC_ROOT}/${name}"

    if [[ ! -d "${dir}/.git" ]]; then
        log "Cloning ${name}"

        git clone --recursive "$repo" "$dir"
    else
        log "Updating ${name}"

        git -C "$dir" fetch --all --tags --prune

        # Keep the local checkout simple: this script is intended to track
        # upstream master/main rather than preserve local modifications.
        git -C "$dir" reset --hard
        git -C "$dir" clean -fd

        case "$name" in
            yosys|nextpnr|verilator)
                git -C "$dir" checkout -B master origin/master
                ;;
            prjtrellis)
                git -C "$dir" checkout -B main origin/main
                ;;
        esac

        git -C "$dir" submodule update --init --recursive
    fi

    printf "    %s: " "$name"
    git -C "$dir" describe --always --dirty --tags 2>/dev/null || true
}

###############################################################################
# Yosys
###############################################################################

build_yosys()
{
    local dir="${SRC_ROOT}/yosys"

    clone_or_update yosys "$YOSYS_REPO"

    log "Building Yosys"

    cmake \
        -S "$dir" \
        -B "$dir/build" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$PREFIX"

    cmake \
        --build "$dir/build" \
        --config Release \
        --parallel "$JOBS"

    sudo cmake \
        --install "$dir/build" \
        --strip
}

###############################################################################
# Verilator
###############################################################################

build_verilator()
{
    local dir="${SRC_ROOT}/verilator"

    clone_or_update verilator "$VERILATOR_REPO"

    log "Building Verilator"

    pushd "$dir" >/dev/null

    unset VERILATOR_ROOT

    autoconf

    ./configure \
        --prefix="$PREFIX"

    make -j"$JOBS"

    sudo make install

    popd >/dev/null
}

###############################################################################
# Project Trellis
###############################################################################

build_trellis()
{
    local dir="${SRC_ROOT}/prjtrellis"

    clone_or_update prjtrellis "$TRELLIS_REPO"

    log "Building Project Trellis / libtrellis"

    pushd "$dir/libtrellis" >/dev/null

    # Project Trellis currently documents an in-tree libtrellis build.
    cmake \
        . \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$PREFIX"

    make -j"$JOBS"

    sudo make install

    popd >/dev/null

    # Refresh the dynamic linker cache when libtrellis was installed into
    # /usr/local.
    if [[ "$PREFIX" == "/usr/local" ]]; then
        sudo ldconfig
    fi
}

###############################################################################
# nextpnr-ecp5
###############################################################################

build_nextpnr()
{
    local dir="${SRC_ROOT}/nextpnr"

    clone_or_update nextpnr "$NEXTPNR_REPO"

    log "Building nextpnr-ecp5"

    cmake \
        -S "$dir" \
        -B "$dir/build" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$PREFIX" \
        -DARCH=ecp5 \
        -DTRELLIS_INSTALL_PREFIX="$PREFIX"

    cmake \
        --build "$dir/build" \
        --config Release \
        --parallel "$JOBS"

    sudo cmake \
        --install "$dir/build" \
        --strip
}

###############################################################################
# Determine what needs building
###############################################################################

have_command()
{
    command -v "$1" >/dev/null 2>&1
}

NEED_YOSYS=0
NEED_VERILATOR=0
NEED_TRELLIS=0
NEED_NEXTPNR=0

if [[ "$FORCE_REBUILD" == "1" ]] || ! have_command yosys; then
    NEED_YOSYS=1
fi

if [[ "$FORCE_REBUILD" == "1" ]] || ! have_command verilator; then
    NEED_VERILATOR=1
fi

if [[ "$FORCE_REBUILD" == "1" ]] ||
   ! have_command nextpnr-ecp5 ||
   ! have_command ecppack; then
    NEED_TRELLIS=1
    NEED_NEXTPNR=1
fi

###############################################################################
# Build
###############################################################################

if [[ "$NEED_YOSYS" == "1" ]]; then
    build_yosys
fi

if [[ "$NEED_VERILATOR" == "1" ]]; then
    build_verilator
fi

# nextpnr-ecp5 depends on libtrellis, so Trellis is always rebuilt first when
# nextpnr needs to be installed/rebuilt.
if [[ "$NEED_TRELLIS" == "1" ]]; then
    build_trellis
fi

if [[ "$NEED_NEXTPNR" == "1" ]]; then
    build_nextpnr
fi

###############################################################################
# Final report
###############################################################################

hash -r

log "Final toolchain versions"

report_toolchain ||
    die "One or more required tools are still missing after installation."

cat <<EOF

Installation complete.

Source tree:
    $SRC_ROOT

Install prefix:
    $PREFIX

Tools:
    yosys
    verilator
    nextpnr-ecp5
    ecppack

Run this script again at any time. If the commands above exist, it will only
report their versions and will not rebuild them.

To explicitly rebuild from the current upstream repositories:

    FORCE_REBUILD=1 $0
EOF

