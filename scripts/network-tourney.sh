#!/bin/sh
for net in $1/*
do
    EXE="frzn-$(basename $net .ckpt)"
    echo Building $EXE
    scripts/trainer.py dump $net
    make EXE=$EXE
    ENGINES="$ENGINES -engine name=$EXE cmd=./$EXE"
done
cutechess-cli -repeat -games 2 -rounds 100 -tb ~/syzygy/ -resign movecount=5 score=500 -draw movenumber=40 movecount=5 score=15 -concurrency 16 -each proto=uci tc=8+0.08 -openings file=$HOME/4moves_noob.epd format=epd order=random $ENGINES
