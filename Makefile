# OB compliance makefile
# To build normally, use cargo build --release

ifndef EXE
$(error You do not appear to be OpenBench - please use cargo instead)
endif

EVALFILE = frozenight/model.json.zst
EXE = Frozenight
ifeq ($(OS),Windows_NT)
	NAME := $(EXE).exe
else
	NAME := $(EXE)
endif

all:
	EVALFILE=$(EVALFILE) cargo build --release --bin frozenight-uci
	mv target/release/frozenight-uci $(NAME)
