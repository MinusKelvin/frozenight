#!/bin/sh
for net in $1/*
do
    EXE="frzn-$(basename $net .ckpt)"
    echo Building $EXE
    scripts/trainer.py dump $net
    make EXE=$EXE
    ENGINES="$ENGINES -engine name=$EXE cmd=./$EXE"
done
cutechess-cli -repeat -games 2 -rounds 50 -tb ~/syzygy/ -resign movecount=5 score=500 -draw movenumber=40 movecount=5 score=15 -concurrency 32 -each proto=uci tc=1+0.01 -openings file=$HOME/4moves_noob.epd format=epd order=random $ENGINES
