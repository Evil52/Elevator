#!/usr/bin/env bash
# Те же проверки, что в CI (fmt, clippy, test), но на самособранном Ferrocene.
# Тулчейн подключается один раз:
#   rustup toolchain link ferrocene-selfbuild ~/ferrocene-build/install
# Другое имя тулчейна: FERROCENE_TOOLCHAIN=имя scripts/check-ferrocene.sh
set -euo pipefail

TC="${FERROCENE_TOOLCHAIN:-ferrocene-selfbuild}"
cd "$(dirname "$0")/.."

if ! rustup toolchain list | cut -d' ' -f1 | grep -qx "$TC"; then
    echo "Тулчейн '$TC' не найден. Подключи сборку:" >&2
    echo "  rustup toolchain link $TC ~/ferrocene-build/install" >&2
    exit 1
fi

version="$(rustc "+$TC" --version)"
case "$version" in
    *Ferrocene*) ;;
    *) echo "Тулчейн '$TC' не Ferrocene: $version" >&2; exit 1 ;;
esac
echo "==> $version"

echo "==> fmt"
cargo "+$TC" fmt --all --check
echo "==> clippy"
cargo "+$TC" clippy --workspace --all-targets -- -D warnings
echo "==> test"
cargo "+$TC" test --workspace
echo "==> OK"
