# OB compliance makefile
# To build normally, use cargo build --release

EXE = Frozenight
ifeq ($(OS),Windows_NT)
	NAME := $(EXE).exe
else
	NAME := $(EXE)
endif

all:
	cargo rustc --release -p frozenight-uci -- -C target-cpu=native --emit link=$(NAME)
