# Runs the Verilator testbenches and the Yosys synthesis report for Xilinx FPGAs.
# Usage: make [all|fetch|decode|execute|writeback|regfile|synth|clean]

VERILATOR ?= verilator
YOSYS ?= yosys
TB := tb

# Each file holds one module. The module name is vea_ and the file name.
RTL_FILES := alu fetch decode execute writeback regfile

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

.PHONY: all fetch decode execute writeback regfile synth clean

# The run continues after a failure, so one run shows every testbench result.
all:
	@$(MAKE) --keep-going fetch decode execute writeback regfile

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
	    alu.sv execute.sv $(TB)/tb_execute.sv > /dev/null \
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

# Gets cell counts for each module (roughly). Every file is read every time.
synth:
	@for m in $(RTL_FILES); do \
	  echo "== synth $$m.sv ($(XILINX_FAMILY))"; \
	  $(YOSYS) -q -p "read_verilog $(YOSYS_READ_FLAGS) $(RTL_FILES:%=%.sv); \
	    synth_xilinx $(YOSYS_SYNTH_FLAGS) -top vea_$$m; \
	    tee -o /dev/stdout stat -tech xilinx" \
	    || exit 1; \
	done

clean:
	rm -rf build
