#!/bin/sh

if ! git diff --exit-code --quiet
then
    echo "Working directory is not clean; cannot generate bench" >&2
    exit 1
fi

NODES=$(cargo run bench | awk '{print $1}')
echo >>"$1"
echo "bench: $NODES" >>"$1"
