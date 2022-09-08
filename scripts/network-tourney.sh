#!/bin/sh

CUTECHESS_ARGS="-repeat -recover -games 5000 -tournament knockout -concurrency 16"
CUTECHESS_ARGS="$CUTECHESS_ARGS -openings file=4moves_noob.epd format=epd order=random"
CUTECHESS_ARGS="$CUTECHESS_ARGS -draw movenumber=40 movecount=5 score=10"
CUTECHESS_ARGS="$CUTECHESS_ARGS -resign movecount=4 score=500"
CUTECHESS_ARGS="$CUTECHESS_ARGS -each nodes=10000 proto=uci"

mkdir -p builds
for net in nn/*.json; do
    scripts/json-to-frozenight.py "$net" ../frozenight/frozenight/model.rs
    cargo build --release
    NETVER=`$(basename "$net" .json)`
    cp ../frozenight/target/release/frozenight-uci builds/$NETVER
    CUTECHESS_ARGS="$CUTECHESS_ARGS -engine name=$NETVER cmd=builds/$NETVER"
done

FIND_WINNER=<<-AWK
{
    if (match($0, /^\t+/) && RLENGTH > best) {
        best = RLENGTH;
        engine = $1;
    }
}
END {
    print "WINNER:", engine;
}
AWK

cutechess-cli $CUTECHESS_ARGS | tee >(awk "$FIND_WINNER")
