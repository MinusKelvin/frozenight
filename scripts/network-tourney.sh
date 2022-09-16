#!/bin/bash
set -e

CUTECHESS_ARGS="-repeat -recover -games 1000 -tournament knockout -concurrency 16"
CUTECHESS_ARGS="$CUTECHESS_ARGS -openings file=$HOME/4moves_noob.epd format=epd order=random"
CUTECHESS_ARGS="$CUTECHESS_ARGS -draw movenumber=40 movecount=5 score=10"
CUTECHESS_ARGS="$CUTECHESS_ARGS -resign movecount=4 score=500"
CUTECHESS_ARGS="$CUTECHESS_ARGS -each nodes=32000 proto=uci tc=inf"

[ -e .tmp-builds ] && rm -r .tmp-builds
[ -e .tmp-networks ] && rm -r .tmp-networks
mkdir -p .tmp-builds .tmp-networks
tar --zstd -xf "$1" -C .tmp-networks
for net in .tmp-networks/*.json; do
    EVALFILE="$net" cargo build --release --bin frozenight-uci
    NETVER=$(basename "$net" .json)
    cp target/release/frozenight-uci .tmp-builds/$NETVER
    CUTECHESS_ARGS="$CUTECHESS_ARGS -engine name=$NETVER cmd=.tmp-builds/$NETVER"
done

FIND_WINNER=$(cat <<AWK
{
    if (match(\$0, /^\\t+/) && RLENGTH > best) {
        best = RLENGTH;
        engine = \$1;
    }
}
END {
    print engine;
}
AWK
)

WINNER=$(cutechess-cli $CUTECHESS_ARGS | tee -a /dev/stderr | awk "$FIND_WINNER")
cp ".tmp-networks/$WINNER.json" `dirname "$1"`/`basename "$1" .tar.zst`.json
echo winner: $WINNER

rm -r .tmp-builds .tmp-networks
