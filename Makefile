# OB compliance makefile
# To build normally, use cargo build --release

ifndef EXE
$(error You do not appear to be OpenBench - please use cargo instead)
endif

EXE = Frozenight
ifeq ($(OS),Windows_NT)
	NAME := $(EXE).exe
else
	NAME := $(EXE)
endif

export RUSTFLAGS=-C target-cpu=native

all:
	cargo build --release --bin frozenight-uci --features ob-no-adjudicate
	mv target/release/frozenight-uci $(NAME)
