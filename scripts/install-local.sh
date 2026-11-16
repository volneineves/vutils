#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
    if ! command -v curl >/dev/null 2>&1; then
        echo 'Instale curl para baixar o Rust e execute novamente.' >&2
        exit 1
    fi
    installer="$(mktemp)"
    trap 'rm -f -- "$installer"' EXIT
    echo 'Instalando Rust stable com o perfil mínimo…'
    curl --proto '=https' --tlsv1.2 --fail --location \
        https://sh.rustup.rs --output "$installer"
    sh "$installer" -y --profile minimal --default-toolchain stable --no-modify-path
fi

echo 'Compilando e instalando esta cópia local do vutils…'
cargo install --path "$project_dir" --locked --root "$HOME/.local" --force
"$HOME/.local/bin/vutils" --version
printf '\nAbra a TUI com:\n  %s/.local/bin/vu tui\n\nUse Ctrl-P para buscar ferramentas e F5 para executar.\n' "$HOME"
