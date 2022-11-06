#!/bin/bash

CUTECHESS_ARGS="-repeat -recover -games 2 -concurrency 16"
CUTECHESS_ARGS="$CUTECHESS_ARGS -openings file=$HOME/4moves_noob.epd format=epd order=random"
CUTECHESS_ARGS="$CUTECHESS_ARGS -draw movenumber=40 movecount=5 score=10"
CUTECHESS_ARGS="$CUTECHESS_ARGS -resign movecount=4 score=500"
CUTECHESS_ARGS="$CUTECHESS_ARGS -each nodes=32000 proto=uci tc=inf"

[ -e .tmp-builds ] && rm -r .tmp-builds
[ -e .tmp-networks ] && rm -r .tmp-networks
mkdir -p .tmp-builds .tmp-networks

CUTOFF="$1"
shift 1

let COUNT=0
for net in "$@"; do
    NET_NAME=`basename $net .tar.zst`
    tar --zstd -xf "$net" -C .tmp-networks
    for n in .tmp-networks/*.json; do
        NAME=$NET_NAME-`basename $n .json`
        zstd -19 --rm $n -o .tmp-networks/$NAME.json.zst
    done
done

let i=$CUTOFF
while true; do
    if [ ! -f .tmp-networks/*-$i.json.zst ]; then
        break
    fi
    for ef in .tmp-networks/*-$i.json.zst; do
        EVALFILE="$ef" cargo build --release --bin frozenight-uci
        NAME=`basename $ef .json.zst`
        cp target/release/frozenight-uci .tmp-builds/$NAME
        CUTECHESS_ARGS="$CUTECHESS_ARGS -engine name=$NAME cmd=.tmp-builds/$NAME"
        let COUNT++
    done
    let i++
done

let BY=$COUNT-1
let "ROUNDS = (2000 + $BY-1) / $BY"

WINNER=$(nice cutechess-cli $CUTECHESS_ARGS -rounds $ROUNDS | tee -a /dev/stderr | awk '/^\s*1\s/ { print($2) }' )
cp .tmp-networks/$WINNER.json.zst .
