# Runs the Verilator testbenches and the Yosys synthesis / nextpnr-ecp5 place-and-route
# reports for ECP5 FPGAs.
# Usage: make [all|fetch|decode|execute|writeback|regfile|mem_if|core|conform|run|waves|synth|pnr|clean]
# pnr takes USE_LPF, PNR_FREQ, PNR_LIVE and PNR_ARGS. See below.

VERILATOR   ?= verilator
YOSYS       ?= yosys
NEXTPNR_ECP5 ?= nextpnr-ecp5
TB := tb

VEA_SIM_DIR := ../vea_sim
VEA_SIM_BIN := $(VEA_SIM_DIR)/target/release/vea_sim

# Each file holds one module. The module name is vea_ and the file name, except
# vea_core, whose file already carries the vea_ prefix.
RTL_FILES := alu fetch decode execute writeback regfile mem_if mul
# Package files. Every read_verilog call needs these first; none is a synth top.
PKG_FILES := vea_pkg

# Board files. These hold the pins and the PLL of one board, not the core. They stay out
# of RTL_FILES, because vea_pll holds an EHXPLLL block. That block is a hard block of the
# FPGA, and Verilator has no model of it, so the testbenches cannot read these files.
BOARD_FILES := vea_pll vea_board_top

# With -noautowire, a name that is not declared is an error. Without it, Yosys makes a
# new wire (or latch!) for the name.
YOSYS_READ_FLAGS  := -sv -noautowire

# The nextpnr-ecp5 device flag (see `nextpnr-ecp5 --help`, "Architecture specific
# options"), without the leading --. 12k is the LFE5U-12F, the board target. One
# nextpnr-ecp5 binary covers every ECP5 part, so switching devices is just this flag,
# no rebuild.
# Other values: 25k, 45k, 85k, um5g-85k, etc. Override with make pnr ECP5_DEVICE=45k
#
# Project Trellis gives the LFE5U-12F and the LFE5U-25F the same database, because the
# two parts use the same die. So a 12k run reports the resources of the 25F: 24288 LUTs
# and 56 EBR. The LFE5U-12F data sheet limits are lower, near 12144 LUTs and 32 EBR.
# Read the utilisation table against the data sheet, not against the printed total.
ECP5_DEVICE ?= 12k

# The nextpnr-ecp5 package flag. CABGA256 is the board target, the 14x14 mm 256-ball
# package of the LFE5U-12F-6BG256C. It gives 197 user pins.
# Other values: CABGA381, CABGA554, CSFBGA285, etc.
# Usage: make pnr ECP5_PACKAGE=CABGA381
ECP5_PACKAGE ?= CABGA256

# The nextpnr-ecp5 speed grade flag, 6, 7 or 8. 6 is the slowest grade and the grade of
# the LFE5U-12F-6BG256C, so it keeps the Fmax result on the safe side.
# Usage: make pnr ECP5_SPEED=7
ECP5_SPEED ?= 6

# What make pnr places and routes.
# Empty (the default): vea_pnr_top. It keeps only the clock and the SPI pins, so the pin
# routes stay short. Use it to measure the core.
# A file name: vea_board_top, the real board. That LPF file maps the ports to balls, and
# it also holds the SYSCONFIG settings of the bitstream.
# Usage: make pnr USE_LPF=vea_board.lpf
USE_LPF ?=
PNR_TOP  := $(if $(USE_LPF),vea_board_top,vea_pnr_top)
PNR_JSON := build/$(PNR_TOP).json

# Target frequency of place and route, in MHz. Empty keeps the nextpnr default of 12 MHz.
# A target makes the placer and router timing-driven. This is much slower: minutes, not
# seconds. A missed target does not fail the run. The log shows the Fmax that it reached.
# Usage: make pnr PNR_FREQ=100
PNR_FREQ ?=

# Set to 1 to print the nextpnr log to the terminal while it runs. The file
# build/pnr.log is written in both cases.
# Usage: make pnr PNR_LIVE=1
PNR_LIVE ?=

# More nextpnr-ecp5 flags. See `nextpnr-ecp5 --help`. For example --verbose, --debug-placer,
# --debug-router or --seed 3.
# Usage: make pnr PNR_ARGS="--seed 3"
PNR_ARGS ?=

PNR_FLAGS := --package $(ECP5_PACKAGE) --json $(PNR_JSON) --$(ECP5_DEVICE) \
  --parallel-refine --tmg-ripup --speed $(ECP5_SPEED) \
  $(if $(USE_LPF),--lpf $(USE_LPF)) --lpf-allow-unconstrained \
  --placed-svg ./build/placement.svg --routed-svg ./build/routing.svg \
  $(if $(PNR_FREQ),--freq $(PNR_FREQ) --timing-allow-fail) $(PNR_ARGS)

# With -flatten, Yosys can optimize across module borders, supposedly.
#											|		  |			|
YOSYS_SYNTH_FLAGS := -flatten -noiopad -widelut

# The tests do not lint the units. Lint is a separate step.
VFLAGS := --binary --timing --timescale 1ns/1ps -Wno-fatal -Wno-lint -Wno-style -Wno-TIMESCALEMOD -j

# A latency of 1 hides a stale reply after a redirect, so Fetch also runs with slower
# memory. The ready pattern tests a memory that is not always ready for a request.
FETCH_LATENCIES := 1 2 3 4
FETCH_TOGGLES   := 0 1

.PHONY: all alu fetch decode execute writeback regfile mem_if core run waves conform synth pnr clean

# The run continues after a failure, so one run shows every testbench result.
all:
	@$(MAKE) --keep-going alu fetch decode execute writeback regfile mem_if core conform

alu:
	@mkdir -p build; rc=0; \
	echo "== alu"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_alu --Mdir build/alu \
	    alu.sv $(TB)/tb_alu.sv > /dev/null \
	  && ./build/alu/Vtb_alu || rc=1; \
	echo "== adder"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_adder --Mdir build/adder \
	    alu.sv $(TB)/tb_adder.sv > /dev/null \
	  && ./build/adder/Vtb_adder || rc=1; \
	echo "== mul"; \
	  $(VERILATOR) $(VFLAGS)  --top-module tb_mul --Mdir build/mul \
	    mul.sv $(TB)/tb_mul.sv > /dev/null \
	  && ./build/mul/Vtb_mul || rc=1; \
	exit $$rc

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
	    vea_pkg.sv alu.sv mul.sv execute.sv $(TB)/tb_execute.sv > /dev/null \
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
	    $(TB)/sim_fast_mem.sv $(TB)/tb_core.sv > /dev/null \
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
	  $(TB)/sim_fast_mem.sv $(TB)/tb_core.sv > /dev/null
	@./build/run_core/Vtb_core +prog=build/run.hex

# Same as `run`, but with a waveform dump, opened in GTKWave. Needs cargo and gtkwave.
# Usage: make waves SRC=../vea_sim/tests/sample_basics.s
waves:
	@test -n "$(SRC)" || { echo "usage: make waves SRC=path/to/prog.s"; exit 1; }
	@mkdir -p build
	@cargo build --release --manifest-path $(VEA_SIM_DIR)/Cargo.toml -q
	@$(VEA_SIM_BIN) asm $(SRC) > build/waves.hex
	@$(VERILATOR) $(VFLAGS) --trace-vcd --top-module tb_core --Mdir build/waves_core \
	  $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv $(TB)/tb_spi_mem.sv \
	  $(TB)/sim_fast_mem.sv $(TB)/tb_core.sv > /dev/null
	@./build/waves_core/Vtb_core +prog=build/waves.hex +trace=build/wave.vcd
	gtkwave build/wave.vcd

# run_all_instrs_all_modes uses div. vea_alu has no divide silicon yet (see its own TODO
# comment). The RTL cannot run this program yet, so it is skipped here.
# vea_sim's own `cargo test` still covers this program in full.
CONFORM_SKIP := run_all_instrs_all_modes

# Builds the vea_sim/tests/*.s corpus: a hex image and a `vea_sim conform` expected-
# register file, for each program. Runs the whole corpus through vea_core in one pass
# (tb/tb_core.sv's +conform_dir mode).
# The RTL and the reference simulator must agree on every register, not just the ones
# a `;=` checkpoint names. This target needs cargo, like `run` does.
# Builds with SIM_FAST_MEM=1 (see vea_core.sv): this corpus checks pipeline and ISA
# correctness, not the SPI wire protocol, so it skips the real, slow SPI wait on every
# load and store. tb_mem_if.sv still checks the real protocol, at real speed.
conform:
	@mkdir -p build/conform
	@cargo build --release --manifest-path $(VEA_SIM_DIR)/Cargo.toml -q
	@rm -f build/conform/manifest.txt
	@for f in $(VEA_SIM_DIR)/tests/*.s; do \
	  name=$$(basename $$f .s); \
	  skip=0; for s in $(CONFORM_SKIP); do [ "$$name" = "$$s" ] && skip=1; done; \
	  [ "$$skip" = 1 ] && continue; \
	  $(VEA_SIM_BIN) asm $$f > build/conform/$$name.hex || exit 1; \
	  $(VEA_SIM_BIN) conform $$f > build/conform/$$name.expect || exit 1; \
	  echo $$name >> build/conform/manifest.txt; \
	done
	@echo "== conform ($$(wc -l < build/conform/manifest.txt) programs)"
	@$(VERILATOR) $(VFLAGS) -GSIM_FAST_MEM=1 --top-module tb_core --Mdir build/conform_core \
	  $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv $(TB)/tb_spi_mem.sv \
	  $(TB)/sim_fast_mem.sv $(TB)/tb_core.sv > /dev/null
	@./build/conform_core/Vtb_core +conform_dir=build/conform

# Gets cell counts for each module (roughly). Every file is read every time.
synth:
	@for m in $(RTL_FILES); do \
	  echo "== synth $$m.sv (ecp5)"; \
	  $(YOSYS) -q -p "read_verilog $(YOSYS_READ_FLAGS) $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv; \
	    synth_ecp5 $(YOSYS_SYNTH_FLAGS) -top vea_$$m; \
	    tee -o /dev/stdout stat" \
	    || exit 1; \
	done
	@echo "== synth vea_core.sv (ecp5)"; \
	  $(YOSYS) -q -p "read_verilog $(YOSYS_READ_FLAGS) $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv; \
	    synth_ecp5 $(YOSYS_SYNTH_FLAGS) -top vea_core; \
	    tee -o /dev/stdout stat" \
	  || exit 1

# Places and routes PNR_TOP for ECP5_DEVICE. Prints the device utilisation table, the
# clock-to-clock critical path (the synchronous logic that caps max frequency; the
# cross-domain paths to/from unconstrained I/O are noise until there is an LPF file, so
# they are left out), and the max frequency result. nextpnr's full log (louder still:
# --no-print-critical-path-source only drops the file/line source annotations) goes to
# build/pnr.log instead of the terminal. Without USE_LPF, the top is vea_pnr_top, which has
# only 5 pins, and nextpnr-ecp5 auto-assigns them. With USE_LPF, the top is vea_board_top. Any
# port that the LPF file does not constrain gets an automatic pin.
pnr:
	@mkdir -p build
	@echo "== synth_ecp5 $(PNR_TOP) ($(ECP5_DEVICE))"
	@$(YOSYS) -q -p "read_verilog $(YOSYS_READ_FLAGS) $(PKG_FILES:%=%.sv) $(RTL_FILES:%=%.sv) vea_core.sv vea_pnr_top.sv $(BOARD_FILES:%=%.sv); \
	  synth_ecp5 $(YOSYS_SYNTH_FLAGS) -top $(PNR_TOP) -json $(PNR_JSON)"
	@echo "== nextpnr-ecp5 $(PNR_TOP) ($(ECP5_DEVICE) $(ECP5_PACKAGE) -$(ECP5_SPEED), pins: $(if $(USE_LPF),$(USE_LPF),none), target $(if $(PNR_FREQ),$(PNR_FREQ) MHz,12 MHz default))"
	@$(NEXTPNR_ECP5) --test
	@if [ -n "$(PNR_LIVE)" ]; then \
	    $(NEXTPNR_ECP5) $(PNR_FLAGS) --log build/pnr.log; rc=$$?; \
	  else \
	    $(NEXTPNR_ECP5) $(PNR_FLAGS) > build/pnr.log 2>&1; rc=$$?; \
	  fi; \
	  awk '/^Info: Device utilisation:/ {p=1} p {sub(/^Info: ?/, ""); print} p && /^$$/ {p=0}' build/pnr.log; \
	  echo; \
	  awk '/Critical path report for clock .* \(posedge -> posedge\):/ {p=1} \
	       p {sub(/^Info: ?/, ""); print} \
	       p && /ns logic, .* ns routing/ {p=0}' build/pnr.log; \
	  echo; \
	  grep -E "^Info: Max (frequency|delay)" build/pnr.log | sed 's/^Info: //'; \
	  echo; grep -E "^[0-9]+ warning" build/pnr.log; \
	  test $$rc -eq 0 || { echo "nextpnr-ecp5 failed, see build/pnr.log"; exit $$rc; }

clean:
	rm -rf build
