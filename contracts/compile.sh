#!/usr/bin/env bash

ROOT_DIR="$(cd "$( dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd )"
cd "$ROOT_DIR"

check_truffle() {
    if [[ -n "$TRUFFLE_BIN" ]]; then
        return
    fi

    bin="$1"
    if [[ -x "$bin" ]] && "$bin" version >/dev/null 2>&1; then
        TRUFFLE_BIN="$bin"
    fi
}

check_truffle "$ROOT_DIR/node_modules/.bin/truffle"
check_truffle "$HOME/.npm/bin/truffle"
check_truffle "/usr/local/bin/truffle"
check_truffle "/usr/bin/truffle"
check_truffle "$(which truffle)"

if [[ -z "$TRUFFLE_BIN" ]]; then
    npm install truffle
    check_truffle "$ROOT_DIR/node_modules/.bin/truffle"
fi

if [[ -z "$TRUFFLE_BIN" ]]; then
    echo "Please install truffle." >&2
    exit 1
fi

"$TRUFFLE_BIN" compile
