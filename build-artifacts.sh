#!/bin/sh

VERSION=$(cargo pkgid frozenight-uci | grep -Eo '[^:]+$')

build() {
    RUSTFLAGS="-C target-cpu=$2" cargo build --release --bin frozenight-uci --target $1
    mv target/$1/release/frozenight-uci$4 frozenight-$VERSION-$3-$2$4
}

build_x86_64() {
    build $1 x86-64 $2 $3
    build $1 x86-64-v2 $2 $3
    build $1 x86-64-v3 $2 $3
}

build_x86_64 x86_64-unknown-linux-gnu linux ''
build_x86_64 x86_64-pc-windows-gnu windows .exe
