#!/usr/bin/env bash
# Asm spot-check for the constant-time signing path.
#
# Emits release assembly, then counts `cmov` and secret-relevant
# conditional jumps in the CT primitives. Fails if either count
# regresses beyond the documented baseline recorded in the project
# audit log (PHASE_4_BACKLOG.md or equivalent).
#
# Run this after every rustc upgrade (rust-toolchain.toml pin bump)
# or any change to the CT primitives (src/core/mladder.rs,
# src/core/scalar.rs ct_select_*, src/core/gf2m_257.rs::freduce_257,
# src/core/sign.rs::truncate).
#
# Usage:  scripts/asm_ct_check.sh
# Exit:   0 on pass, 1 on regression.

set -euo pipefail

cd "$(dirname "$0")/.."

CARGO="${CARGO:-cargo}"

echo "==> Emitting release asm"
"$CARGO" rustc --release --lib -q -- --emit=asm -C opt-level=3 >/dev/null

ASM="target/release/deps/prro_crypto.s"
if [[ ! -f "$ASM" ]]; then
    echo "ERROR: expected $ASM after asm emit" >&2
    exit 1
fi

# Functions and the branch budget each is allowed. Format: name,max_cmov,max_jcc.
# `jcc` counts include `je/jne/jg/jl/jge/jle/ja/jb/jae/jbe/jz/jnz`.
#
# `mul_base_x_ct` carries a small budget for loop counter branches
# (`extract_bit` bounds check + loop back-edge, both on the public
# 288-iteration counter). The setup/panic paths contribute a handful
# more; 25 is the slack ceiling observed on rustc 1.94.1 release.
#
# `from_fe_truncated` and the scalar CT helpers (`add_mod`, `sub_mod`,
# `mul_mod`) must show zero of both after the Sprint 2.1c5.1b tighten
# and `subtle` wrapping. Any non-zero reading is a regression.
CHECKS=(
    "from_fe_truncated,0,0"
    "add_mod,0,0"
    "sub_mod,0,0"
    "mul_mod,0,0"
    "freduce_257,0,0"
    "mul_base_x_ct,5,25"
)

status=0
for check in "${CHECKS[@]}"; do
    IFS=',' read -r fn max_cmov max_jcc <<<"$check"
    # `head -1` closes the pipe early → grep gets SIGPIPE → writes
    # "Broken pipe" to stderr → pipefail kills the script. Fix: send
    # grep's stderr to /dev/null (the SIGPIPE message is harmless) and
    # append `; true` so the subshell exit code is 0 regardless.
    start=$(grep -nE "^_ZN.*${fn}17h" "$ASM" 2>/dev/null | cut -d: -f1 | head -1 ; true)
    if [[ -z "$start" ]]; then
        printf '%-22s  SKIP (inlined, no standalone symbol)\n' "$fn"
        continue
    fi
    end=$(grep -nE "^\.Lfunc_end" "$ASM" 2>/dev/null | awk -F: -v s="$start" '$1 > s {print $1; exit}' ; true)
    body=$(sed -n "${start},${end}p" "$ASM")
    cmov=$(echo "$body" | grep -cE "^\s*cmov" || true)
    jcc=$(echo "$body" | grep -cE "^\s*(je|jne|jg|jl|jge|jle|ja|jb|jae|jbe|jz|jnz)\s+" || true)

    if (( cmov > max_cmov )) || (( jcc > max_jcc )); then
        printf '%-22s  FAIL  cmov=%d (max %d)  jcc=%d (max %d)\n' \
            "$fn" "$cmov" "$max_cmov" "$jcc" "$max_jcc"
        status=1
    else
        printf '%-22s  ok    cmov=%d jcc=%d\n' "$fn" "$cmov" "$jcc"
    fi
done

if (( status == 0 )); then
    echo "==> asm spot-check: PASS"
else
    echo "==> asm spot-check: REGRESSION — investigate the failing symbol(s)"
    echo "    before accepting the compiler change."
fi
exit "$status"
