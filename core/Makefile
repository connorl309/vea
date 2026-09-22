# Runs the Verilator testbenches and the Yosys synthesis report for Xilinx FPGAs.
# Usage: make [all|fetch|decode|execute|writeback|regfile|mem_if|core|conform|synth|clean]

VERILATOR ?= verilator
YOSYS ?= yosys
TB := tb

VEA_SIM_DIR := ../vea_sim
VEA_SIM_BIN := $(VEA_SIM_DIR)/target/release/vea_sim

# Each file holds one module. The module name is vea_ and the file name, except
# vea_core, whose file already carries the vea_ prefix.
RTL_FILES := alu fetch decode execute writeback regfile mem_if
# Package files. Every read_verilog call needs these first; none is a synth top.
PKG_FILES := vea_pkg

# With -noautowire, a name that is not declared is an error. Without it, Yosys makes a
# new wire (or latch!) for the name.
YOSYS_READ_FLAGS  := -sv -noautowire
# The family selects the LUT and flip-flop cells. xc7 is Series 7 (Artix-7, Kintex-7,
# Virtex-7, Zynq-7000). Other values: make synth XILINX_FAMILY=xcup
XILINX_FAMILY ?= xc7

# With -flatten, Yosys can optimize across module borders, supposedly.
YOSYS_SYNTH_FLAGS := -family $(XILINX_FAMILY) -flatten -noiopad -noclkbuf

# The tests do not lint the units. Lint is a separate step.
VFLAGS := --binary --timing --timescale 1ns/1ps -Wno-fatal -Wno-lint -Wno-style -Wno-TIMESCALEMOD -j

# A latency of 1 hides a stale reply after a redirect, so Fetch also runs with slower
# memory. The ready pattern tests a memory that is not always ready for a request.
FETCH_LATENCIES := 1 2 3 4
FETCH_TOGGLES   := 0 1

.PHONY: all alu fetch decode execute writeback regfile mem_if core run conform synth clean

# The run continues after a failure, so one run shows every testbench result.
all:
	@$(MAKE) --keep-going alu fetch decode execute writeback regfile mem_if core conform

alu:
	@mkdir -p build; echo "== alu"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_alu --Mdir build/alu \
	    alu.sv $(TB)/tb_alu.sv > /dev/null \
	  && ./build/alu/Vtb_alu

fetch:
	@mkdir -p build; rc=0; for l in $(FETCH_LATENCIES); do for t in $(FETCH_TOGGLES); do \
	  echo "== fetch, memory latency $$l, ready toggle $$t"; \
	  $(VERILATOR) $(VFLAGS) -GLATENCY=$$l -GREADY_TOGGLE=$$t  \
	    --top-module tb_fetch --Mdir build/fetch_$$l\_$$t fetch.sv $(TB)/tb_fetch.sv > /dev/null \
	    && ./build/fetch_$$l\_$$t/Vtb_fetch || rc=1; \
	done; done; exit $$rc

decode:
	@mkdir -p build; echo "== decode"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_decode --Mdir build/decode \
	    decode.sv $(TB)/tb_decode.sv > /dev/null \
	  && ./build/decode/Vtb_decode

execute:
	@mkdir -p build; echo "== execute"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_execute --Mdir build/execute \
	    vea_pkg.sv alu.sv execute.sv $(TB)/tb_execute.sv > /dev/null \
	  && ./build/execute/Vtb_execute

writeback:
	@mkdir -p build; echo "== writeback"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_writeback --Mdir build/writeback \
	    writeback.sv $(TB)/tb_writeback.sv > /dev/null \
	  && ./build/writeback/Vtb_writeback

regfile:
	@mkdir -p build; echo "== regfile"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_regfile --Mdir build/regfile \
	    regfile.sv $(TB)/tb_regfile.sv > /dev/null \
	  && ./build/regfile/Vtb_regfile

mem_if:
	@mkdir -p build; echo "== mem_if"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_mem_if --Mdir build/mem_if \
	    vea_pkg.sv mem_if.sv $(TB)/tb_spi_mem.sv $(TB)/tb_mem_if.sv > /dev/null \
	  && ./build/mem_if/Vtb_mem_if

core:
	@mkdir -p build; echo "== core"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_core --Mdir build/core \
	    $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv $(TB)/tb_spi_mem.sv \
	    $(TB)/tb_core.sv > /dev/null \
	  && ./build/core/Vtb_core

# Assembles $(SRC) with the vea_sim CLI and runs it through vea_core to halt, dumping
# every register (tb/tb_core.sv's +prog handling). Needs cargo; every other target in
# this Makefile does not.
# Usage: make run SRC=../vea_sim/tests/sample_basics.s
run:
	@test -n "$(SRC)" || { echo "usage: make run SRC=path/to/prog.s"; exit 1; }
	@mkdir -p build
	@cargo build --release --manifest-path $(VEA_SIM_DIR)/Cargo.toml -q
	@$(VEA_SIM_BIN) asm $(SRC) > build/run.hex
	@$(VERILATOR) $(VFLAGS) --top-module tb_core --Mdir build/run_core \
	  $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv $(TB)/tb_spi_mem.sv \
	  $(TB)/tb_core.sv > /dev/null
	@./build/run_core/Vtb_core +prog=build/run.hex

# Builds the vea_sim/tests/*.s corpus (hex image + `vea_sim conform`'s expected final
# registers, per program) and runs the whole thing through vea_core in one pass
# (tb/tb_core.sv's +conform_dir handling): the RTL and the reference simulator must
# agree on every register a program ends with, not just the ones a `;=` checkpoint
# names. Needs cargo, like `run`.
conform:
	@mkdir -p build/conform
	@cargo build --release --manifest-path $(VEA_SIM_DIR)/Cargo.toml -q
	@rm -f build/conform/manifest.txt
	@for f in $(VEA_SIM_DIR)/tests/*.s; do \
	  name=$$(basename $$f .s); \
	  $(VEA_SIM_BIN) asm $$f > build/conform/$$name.hex || exit 1; \
	  $(VEA_SIM_BIN) conform $$f > build/conform/$$name.expect || exit 1; \
	  echo $$name >> build/conform/manifest.txt; \
	done
	@echo "== conform ($$(wc -l < build/conform/manifest.txt) programs)"
	@$(VERILATOR) $(VFLAGS) --top-module tb_core --Mdir build/conform_core \
	  $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv $(TB)/tb_spi_mem.sv \
	  $(TB)/tb_core.sv > /dev/null
	@./build/conform_core/Vtb_core +conform_dir=build/conform

# Gets cell counts for each module (roughly). Every file is read every time.
synth:
	@for m in $(RTL_FILES); do \
	  echo "== synth $$m.sv ($(XILINX_FAMILY))"; \
	  $(YOSYS) -q -p "read_verilog $(YOSYS_READ_FLAGS) $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv; \
	    synth_xilinx $(YOSYS_SYNTH_FLAGS) -top vea_$$m; \
	    tee -o /dev/stdout stat -tech xilinx" \
	    || exit 1; \
	done
	@echo "== synth vea_core.sv ($(XILINX_FAMILY))"; \
	  $(YOSYS) -q -p "read_verilog $(YOSYS_READ_FLAGS) $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv; \
	    synth_xilinx $(YOSYS_SYNTH_FLAGS) -top vea_core; \
	    tee -o /dev/stdout stat -tech xilinx" \
	  || exit 1

clean:
	rm -rf build
