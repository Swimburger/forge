#!/usr/bin/env bash
# Compares the parse benchmark of the working tree against a git revision:
#
#     FORGE_SPEC=/path/to/openapi.json benches/compare.sh [REV] [PAIRS]
#
# REV defaults to main and PAIRS to 12. The revision is built in a throwaway
# worktree, so the working tree is left as it is.
#
# Each pair runs the two in ABBA order (base, new, new, base) and takes the
# mean of each side's two runs, so a machine that warms up or cools down
# through the session does not bias one side. Three warm-up pairs are
# discarded first. The statistics are paired, on divan's fastest sample of
# each run. A pair whose four runs differ by more than 10% is reported and
# left out: the machine changed speed in the middle of it.
#
# A difference under about twice its standard error is not one this can see.
set -eu

REV=${1:-main}
PAIRS=${2:-12}
: "${FORGE_SPEC:?set FORGE_SPEC to the document to benchmark}"
export FORGE_SPEC

ROOT=$(git rev-parse --show-toplevel)
WORK=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$WORK/base" > /dev/null 2>&1 || true; rm -rf "$WORK"' EXIT

# Builds the parse benchmark of the tree at $1 into the target folder $2, and
# copies it to $3.
build() {
    (cd "$1" && CARGO_TARGET_DIR=$2 cargo bench --locked --bench parse --no-run 2>&1 | tail -1) >&2
    local bin
    bin=$(ls -t "$2"/release/deps/parse-* | grep -vE '\.(d|o)$' | head -1)
    cp "$bin" "$3"
}

git -C "$ROOT" worktree add --detach "$WORK/base" "$REV" > /dev/null 2>&1
build "$WORK/base" "$WORK/target-base" "$WORK/base.bin"
build "$ROOT" "$ROOT/target" "$WORK/new.bin"

# Writes the fastest sample of a divan run of $1, in ms.
measure() {
    "$1" --bench forge > "$WORK/run.out" 2> /dev/null
    awk '
        /├─ forge/ {f = 1}
        f && /╰─|├─/ && /[0-9] (ms|µs|s) / {
            for (i = 1; i <= NF; i++)
                if ($i ~ /^[0-9]+(\.[0-9]+)?$/) {
                    v = $i; u = $(i+1)
                    print (u == "µs" ? v / 1000 : u == "s" ? v * 1000 : v)
                    exit
                }
        }' "$WORK/run.out"
}

for i in 1 2 3; do
    measure "$WORK/base.bin" > /dev/null; measure "$WORK/new.bin" > /dev/null
done
: > "$WORK/pairs"
for i in $(seq "$PAIRS"); do
    b1=$(measure "$WORK/base.bin"); n1=$(measure "$WORK/new.bin")
    n2=$(measure "$WORK/new.bin"); b2=$(measure "$WORK/base.bin")
    echo "$b1 $n1 $n2 $b2" >> "$WORK/pairs"
    printf 'pair %2d  base %s %s ms  new %s %s ms\n' "$i" "$b1" "$b2" "$n1" "$n2" >&2
done

awk -v rev="$REV" '
    {
        lo = $1; hi = $1
        for (i = 2; i <= 4; i++) { if ($i < lo) lo = $i; if ($i > hi) hi = $i }
        if (hi > lo * 1.1) { printf "pair %d left out: the machine changed speed\n", NR; next }
        n++; b = ($1 + $4) / 2; d = ($2 + $3) / 2 - b
        sb += b; sd += d; sd2 += d * d; if (d > 0) up++
    }
    END {
        if (n < 2) { print "too few pairs to compare"; exit 1 }
        mean = sd / n; se = sqrt((sd2 - n * mean * mean) / (n - 1) / n)
        printf "%s: %.2f ms\n", rev, sb / n
        printf "working tree - %s: %+.3f ms (%+.2f%%), standard error %.3f ms, t = %.1f, slower in %d of %d pairs\n",
            rev, mean, 100 * mean / (sb / n), se, (se > 0 ? mean / se : 0), up, n
    }' "$WORK/pairs"
