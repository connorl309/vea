//! Integration testbench for vea_core. Drives whole programs through imem and checks
//! register file contents and the status outputs.
//!
//! tb_spi_mem.sv plays the SPI SRAM on the data memory pins, so a load or a store now
//! works. Its window is smaller than the real 24-bit address space (see its MEM_BYTES
//! parameter below); an address outside that window wraps.
//!
//! The built-in programs are `vea_sim asm` output for tb_core_progs/*.s, pasted in as
//! byte arrays so this test needs no Rust toolchain at simulation time. Re-run
//! `vea_sim asm <prog>.s` by hand and re-paste if a program or the encoding changes.
//!
//! +prog=<path> (see `make run SRC=...`) instead runs one external program to halt and
//! dumps every register, in place of the built-in suite. Its bytes are read with
//! $readmemh, the format `vea_sim asm`'s plain stdout output is already in. Add
//! +expect=<path> (`vea_sim conform`'s output) to check the dump instead of just
//! printing it.
//!
//! +conform_dir=<dir> (see `make conform`) instead runs the full vea_sim/tests/*.s
//! corpus in one pass, checking each program's halted register file against
//! `vea_sim conform`'s output for that same program: the RTL and the reference
//! simulator must agree on every register, not just the ones a checkpoint names.

module tb_core #(
  parameter int SEED = 0
);
  localparam int MB = 13;

  logic clk   = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  logic                  imem_req_valid;
  logic                  imem_req_ready = 1'b1;
  logic [63:0]            imem_addr;
  logic                  imem_rvalid    = 1'b0;
  logic [8*MB-1:0]       imem_rdata     = '0;

  logic dmem_spi_sck, dmem_spi_cs_n, dmem_spi_mosi, dmem_spi_miso;

  logic halt, err_illegal, err_unsupported, err_trap, err_unaligned;

  vea_core #(.MAX_INSN_BYTES(MB)) dut (.*);

  tb_spi_mem #(.MEM_BYTES(65536)) dmem (
    .spi_sck  (dmem_spi_sck),
    .spi_cs_n (dmem_spi_cs_n),
    .spi_mosi (dmem_spi_mosi),
    .spi_miso (dmem_spi_miso)
  );

  // ---- Instruction memory: always ready, replies one cycle after the request. Fetch's
  // own protocol (latency sweep, ready toggling, drain on redirect) is tb_fetch's job,
  // not this one's; this model only has to be correct, not adversarial.

  logic [7:0] mem [4096];

  // A function, not a task: a prior Verilator quirk on this project delivered an
  // all-zero frame when a task built imem_rdata with part-select writes. Building the
  // whole vector here and assigning it whole avoids that.
  function automatic logic [8*MB-1:0] read_frame(input logic [63:0] a);
    logic [8*MB-1:0] d;
    for (int i = 0; i < MB; i++) d[8*MB-1-8*i -: 8] = mem[int'(a) + i];
    return d;
  endfunction

  always @(posedge clk) begin
    imem_rvalid <= 1'b0;
    if (imem_req_valid) begin
      imem_rvalid <= 1'b1;
      imem_rdata  <= read_frame(imem_addr);
    end
  end

  // ---- Checks ---------------------------------------------------------------------------

  int errors = 0;
  int checks = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_core: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic check_bit(input logic got, input logic want, input string what);
    check64(64'(got), 64'(want), what);
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  function automatic logic [63:0] reg_val(input int n);
    reg_val = dut.u_regfile.regs[n];
  endfunction

  // Directly pokes every register to zero. Only a testbench needs this: regfile.sv
  // deliberately has no reset port (see its own comment - LUT RAM has none, and adding
  // one would force flip-flops), so between directed tests a register an earlier
  // program wrote stays nonzero on purpose (test_trap and test_illegal rely on exactly
  // that to prove a faulted instruction's successor never runs). An arbitrary program
  // checked against vea_sim's output needs the opposite: vea_sim's own regfile starts
  // every run at zero (see regfile.sv's initial block), so its dump is only comparable
  // if this one does too.
  task automatic clear_regfile();
    for (int i = 0; i < 32; i++) dut.u_regfile.regs[i] = '0;
  endtask

  // Clears the program area, loads the new one, and resets the core's pipeline and
  // status latches. The register file is untouched - see clear_regfile().
  task automatic load_program(input logic [7:0] prog[], input int n);
    for (int i = 0; i < 512; i++) mem[i] = 8'h00;
    for (int i = 0; i < n; i++)   mem[i] = prog[i];
    rst_n = 1'b0;
    tick(3);
    rst_n = 1'b1;
  endtask

  // Same as load_program, but for an arbitrary file: `vea_sim asm`'s plain stdout output
  // (hex byte pairs, whitespace-separated) is already $readmemh format. Also clears the
  // register file and data memory, so a check against vea_sim's output (which always
  // starts both at zero) is comparing like with like even when this is not the first
  // program run this sim - a fixed data address one program uses is not guaranteed to
  // be one only it touches.
  task automatic load_program_file(input string path);
    clear_regfile();
    dmem.clear();
    for (int i = 0; i < 4096; i++) mem[i] = 8'h00;
    $readmemh(path, mem);
    rst_n = 1'b0;
    tick(3);
    rst_n = 1'b1;
  endtask

  task automatic dump_regs();
    for (int i = 0; i < 32; i++) $display("tb_core: r%0d = %h", i, reg_val(i));
  endtask

  task automatic run_until_halt(input string name, input int max_cycles);
    int n;
    n = 0;
    while (!halt && n < max_cycles) begin
      tick();
      n++;
    end
    // halt latches the cycle the halt instruction enters Decode's stage-2 register (see
    // decode.sv), which can land on the same edge that also captures writeback's copy of
    // whatever instruction was right in front of it. The register file itself needs one
    // more clock edge beyond that (writeback.sv registers wb_valid before it ever reaches
    // regfile.sv's own write). A fast, no-memory tail hides this: fetch's own latency
    // already has the next frame sitting in Decode by the time the last op completes, so
    // halt does not latch until later anyway. A load as the very last instruction has no
    // such slack, so without this extra tick its write is still in flight when the loop
    // above exits. One idle tick costs nothing once halted, so it always runs.
    if (halt) tick();
    // A fault (see execute.sv's `stopped`) parks the core for good, short of its own
    // halt; that reads identically to a slow program unless the err_ bits are called
    // out here, so a run that never reaches halt still says why in one line.
    if (!halt && (err_illegal || err_unsupported || err_trap || err_unaligned))
      $display("tb_core: %s: stalled on a fault (illegal=%0d unsupported=%0d trap=%0d unaligned=%0d)",
                name, err_illegal, err_unsupported, err_trap, err_unaligned);
    check_bit(halt, 1'b1, {name, ": halted within the cycle budget"});
    if (halt) $display("tb_core: %s halted after %0d cycles", name, n);
  endtask

  // Checks every register against a $readmemh-format expectation file: 32 lines, r0
  // first, the same format `vea_sim conform` prints. That subcommand is the golden
  // oracle: it runs the same program on the onestep simulator to halt and dumps its
  // final regs, so this is a direct RTL-vs-simulator cross-check, not a hand-copied
  // expected value.
  task automatic check_regs_file(input string name, input string path);
    logic [63:0] expected [32];
    $readmemh(path, expected);
    for (int i = 0; i < 32; i++)
      check64(reg_val(i), expected[i], $sformatf("%s: r%0d", name, i));
  endtask

  // A fault stops the core for good (see execute.sv): it never reaches its own halt,
  // and nothing after the faulting instruction ever retires. Runs for a fixed budget
  // and checks halt never asserted, instead of waiting for it.
  task automatic run_and_expect_stuck(input string name, input int cycles);
    tick(cycles);
    check_bit(halt, 1'b0, {name, ": never reaches its own halt once stopped"});
  endtask

  // ---- Programs -------------------------------------------------------------------------
  // See tb_core_progs/*.s for the assembly source.

  // mov, add, sub, and, or, xor, not, shl, shr, mul, halt. No branch, no memory.
  localparam logic [7:0] PROG_ARITH [80] = '{
    8'h01, 8'h41, 8'h01, 8'h0a, 8'h01, 8'h41, 8'h02, 8'h14, 8'h10, 8'h50,
    8'h03, 8'h01, 8'h02, 8'h00, 8'h00, 8'h00, 8'h11, 8'h50, 8'h04, 8'h02,
    8'h01, 8'h00, 8'h00, 8'h00, 8'h12, 8'h50, 8'h05, 8'h01, 8'h02, 8'h00,
    8'h00, 8'h00, 8'h13, 8'h50, 8'h06, 8'h01, 8'h02, 8'h00, 8'h00, 8'h00,
    8'h15, 8'h50, 8'h07, 8'h01, 8'h02, 8'h00, 8'h00, 8'h00, 8'h14, 8'h40,
    8'h08, 8'h01, 8'h16, 8'h51, 8'h09, 8'h01, 8'h02, 8'h00, 8'h00, 8'h00,
    8'h17, 8'h51, 8'h0a, 8'h02, 8'h01, 8'h00, 8'h00, 8'h00, 8'h19, 8'h50,
    8'h0b, 8'h01, 8'h02, 8'h00, 8'h00, 8'h00, 8'hff, 8'h20, 8'h00, 8'h00
  };

  // A countdown loop: sub, cmp, bne, halt. Exercises the redirect path Fetch/Decode take
  // on a taken branch.
  localparam logic [7:0] PROG_BRANCH [24] = '{
    8'h01, 8'h41, 8'h0b, 8'h05, 8'h11, 8'h51, 8'h0b, 8'h0b, 8'h01, 8'h00,
    8'h00, 8'h00, 8'h20, 8'h31, 8'h0b, 8'h00, 8'h30, 8'h3a, 8'hf4, 8'h00,
    8'hff, 8'h20, 8'h00, 8'h00
  };

  // mov, trap, add, halt. TRAP has no target yet, so it must latch err_trap and stop the
  // core: the add and the halt after it must never run.
  localparam logic [7:0] PROG_TRAP [20] = '{
    8'h01, 8'h41, 8'h01, 8'h01, 8'hfe, 8'h30, 8'h05, 8'h00, 8'h10, 8'h50,
    8'h02, 8'h01, 8'h01, 8'h00, 8'h00, 8'h00, 8'hff, 8'h20, 8'h00, 8'h00
  };

  // mov r1,#1; jmp r1; mov r2,#99; halt. The jmp target (1) is off a 4-byte boundary, so
  // it must latch err_unaligned and stop the core: the mov and the halt after it must
  // never run.
  localparam logic [7:0] PROG_UNALIGNED [16] = '{
    8'h01, 8'h41, 8'h01, 8'h01, 8'h31, 8'h30, 8'h01, 8'h00, 8'h01, 8'h41,
    8'h02, 8'h63, 8'hff, 8'h20, 8'h00, 8'h00
  };

  // Not from the assembler: opcode 0x99 is undefined at decode (upper nibble matches no
  // group), so this is a fault by construction. len=2 (opcode+opinfo, no operand bytes),
  // padded to 4. Followed by a normal mov and a halt, to prove the core actually stops:
  // both must never run.
  localparam logic [7:0] PROG_ILLEGAL [12] = '{
    8'h99, 8'h20, 8'h00, 8'h00,
    8'h01, 8'h41, 8'h01, 8'h2a, // mov r1, #42
    8'hff, 8'h20, 8'h00, 8'h00
  };

  // ---- Tests --------------------------------------------------------------------------

  task automatic test_arith();
    load_program(PROG_ARITH, 80);
    run_until_halt("arith", 200);
    check64(reg_val(1),  64'd10,                    "arith: r1");
    check64(reg_val(2),  64'd20,                    "arith: r2");
    check64(reg_val(3),  64'd30,                    "arith: r3 (add)");
    check64(reg_val(4),  64'd10,                    "arith: r4 (sub)");
    check64(reg_val(5),  64'd0,                     "arith: r5 (and)");
    check64(reg_val(6),  64'd30,                    "arith: r6 (or)");
    check64(reg_val(7),  64'd30,                    "arith: r7 (xor)");
    check64(reg_val(8),  64'hFFFF_FFFF_FFFF_FFF5,   "arith: r8 (not)");
    check64(reg_val(9),  64'd40,                    "arith: r9 (shl)");
    check64(reg_val(10), 64'd10,                    "arith: r10 (shr)");
    check64(reg_val(11), 64'd200,                   "arith: r11 (mul)");
    check_bit(err_illegal,     1'b0, "arith: err_illegal stays low");
    check_bit(err_unsupported, 1'b0, "arith: err_unsupported stays low");
    check_bit(err_trap,        1'b0, "arith: err_trap stays low");
    check_bit(err_unaligned,   1'b0, "arith: err_unaligned stays low");
  endtask

  task automatic test_branch();
    load_program(PROG_BRANCH, 24);
    run_until_halt("branch", 200);
    check64(reg_val(11), 64'd0, "branch: r11 counted down to 0");
    check_bit(err_illegal,   1'b0, "branch: err_illegal stays low");
    check_bit(err_unaligned, 1'b0, "branch: err_unaligned stays low");
  endtask

  // The register file has no reset (see regfile.sv), so a register an earlier test
  // wrote stays nonzero here. "Never ran" tests capture the target register first and
  // check it is unchanged, rather than assuming it reads zero.
  task automatic test_trap();
    logic [63:0] r2_before;
    r2_before = reg_val(2);
    load_program(PROG_TRAP, 20);
    run_and_expect_stuck("trap", 100);
    check64(reg_val(1), 64'd1,      "trap: r1 (before the trap)");
    check64(reg_val(2), r2_before,  "trap: r2 unchanged (add after the trap never ran)");
    check_bit(err_trap,      1'b1, "trap: err_trap latched");
    check_bit(err_illegal,   1'b0, "trap: err_illegal stays low");
    check_bit(err_unaligned, 1'b0, "trap: err_unaligned stays low");
  endtask

  task automatic test_unaligned();
    logic [63:0] r2_before;
    r2_before = reg_val(2);
    load_program(PROG_UNALIGNED, 16);
    run_and_expect_stuck("unaligned", 100);
    check64(reg_val(1), 64'd1,     "unaligned: r1");
    check64(reg_val(2), r2_before, "unaligned: r2 unchanged (mov after the jmp never ran)");
    check_bit(err_unaligned, 1'b1, "unaligned: err_unaligned latched");
    check_bit(err_illegal,   1'b0, "unaligned: err_illegal stays low");
  endtask

  task automatic test_illegal();
    logic [63:0] r1_before;
    r1_before = reg_val(1);
    load_program(PROG_ILLEGAL, 12);
    run_and_expect_stuck("illegal", 100);
    check64(reg_val(1), r1_before, "illegal: r1 unchanged (mov after the fault never ran)");
    check_bit(err_illegal,     1'b1, "illegal: err_illegal latched");
    check_bit(err_unsupported, 1'b0, "illegal: err_unsupported stays low");
  endtask

  // +prog=<path> runs one external program instead of the directed suite below: load,
  // run to halt, dump every register, done. Add +expect=<path> (vea_sim conform's
  // output) to also check the dump against it instead of comparing by hand.
  string ext_prog, ext_expect;
  bit    have_ext_prog, have_ext_expect;

  // +conform_dir=<dir> runs the whole vea_sim/tests/*.s corpus: every program named in
  // <dir>/manifest.txt (one name per line, blank lines skipped), loading <dir>/<name>.hex
  // and checking the halted register file against <dir>/<name>.expect. See
  // core/Makefile's `conform` target for how the directory is built.
  task automatic run_conform_one(input string dir, input string name);
    string hex_path, expect_path;
    hex_path    = {dir, "/", name, ".hex"};
    expect_path = {dir, "/", name, ".expect"};
    $display("tb_core: conform %s", name);
    load_program_file(hex_path);
    run_until_halt(name, 400_000);
    // A run that never halted has no final state worth checking; run_until_halt
    // already recorded the one failure that matters (and, on a fault, why).
    if (halt) check_regs_file(name, expect_path);
  endtask

  // Strips $fgets's trailing newline (and a stray \r, in case the manifest was
  // written on a different platform).
  function automatic string chomp(input string s);
    int n;
    n = s.len();
    while (n > 0 && (s[n-1] == "\n" || s[n-1] == "\r")) n--;
    chomp = (n > 0) ? s.substr(0, n-1) : "";
  endfunction

  task automatic run_conform_suite(input string dir);
    int    fd;
    string line, name;
    fd = $fopen({dir, "/manifest.txt"}, "r");
    if (fd == 0) $fatal(1, "tb_core: cannot open conformance manifest in %s", dir);
    while ($fgets(line, fd) != 0) begin
      name = chomp(line);
      if (name.len() > 0) run_conform_one(dir, name);
    end
    $fclose(fd);
  endtask

  string conform_dir;
  bit    have_conform;

  initial begin
    have_ext_prog    = $value$plusargs("prog=%s", ext_prog);
    have_ext_expect  = $value$plusargs("expect=%s", ext_expect);
    have_conform     = $value$plusargs("conform_dir=%s", conform_dir);

    if (have_conform) begin
      $display("tb_core: running conformance suite from %s", conform_dir);
      run_conform_suite(conform_dir);
      $display("tb_core: %0d checks, %0d errors", checks, errors);
      if (errors != 0) $fatal(1, "tb_core failed");
      $finish;
    end

    if (have_ext_prog) begin
      $display("tb_core: running external program %s", ext_prog);
      load_program_file(ext_prog);
      run_until_halt(ext_prog, 400_000);
      dump_regs();
      if (have_ext_expect && halt) check_regs_file(ext_prog, ext_expect);
      $display("tb_core: %0d checks, %0d errors", checks, errors);
      if (errors != 0) $fatal(1, "tb_core failed");
      $finish;
    end

    process::self().srandom(SEED);
    $display("tb_core: seed %0d", SEED);
    $display("tb_core: test_arith");
    test_arith();
    $display("tb_core: test_branch");
    test_branch();
    $display("tb_core: test_trap");
    test_trap();
    $display("tb_core: test_unaligned");
    test_unaligned();
    $display("tb_core: test_illegal");
    test_illegal();

    $display("tb_core: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_core failed");
    $finish;
  end

  initial begin
    #10_000_000;
    $fatal(1, "tb_core watchdog");
  end
endmodule
