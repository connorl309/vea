# Runs the Verilator testbenches. Usage: make [all|alu|fetch|decode|clean]
#
# Each testbench compiles the shared check package first, then the unit,
# then the testbench. The alu.sv file must come before any file that uses
# its macros.

VERILATOR ?= verilator
TB := tb
PKG := $(TB)/tb_util_pkg.sv

# The tests do not lint the units. The lint rules for the units are a separate task.
VFLAGS := --binary --timing --timescale 1ns/1ps -Wno-fatal -Wno-lint -Wno-style -Wno-TIMESCALEMOD

# Fetch runs with more than one memory latency. A latency of 1 hides a stale reply after a redirect.
FETCH_LATENCIES := 1 2 4

.PHONY: all alu fetch decode clean

#  The run continues after a failure, so one run shows every testbench result.
all:
	@$(MAKE) --keep-going alu fetch decode

# Verilator creates the last directory of --Mdir only, not its parent.
build:
	mkdir -p build

alu: | build
	$(VERILATOR) $(VFLAGS) --top-module tb_alu --Mdir build/alu $(PKG) alu.sv $(TB)/tb_alu.sv
	./build/alu/Vtb_alu

fetch: | build
	@rc=0; for l in $(FETCH_LATENCIES); do \
	  echo "== fetch, memory latency $$l"; \
	  $(VERILATOR) $(VFLAGS) -GLATENCY=$$l --top-module tb_fetch --Mdir build/fetch_$$l \
	    $(PKG) fetch.sv $(TB)/tb_imem.sv $(TB)/tb_fetch.sv && ./build/fetch_$$l/Vtb_fetch || rc=1; \
	done; exit $$rc

decode: | build
	$(VERILATOR) $(VFLAGS) --top-module tb_decode --Mdir build/decode $(PKG) decode.sv $(TB)/tb_decode.sv
	./build/decode/Vtb_decode

clean:
	rm -rf build
