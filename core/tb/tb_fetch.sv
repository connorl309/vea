//! Testbench for vea_fetch.
//!
//! The memory model is tb_imem. The LATENCY parameter sets its reply time.
//! The instruction bytes here come from the assembler test vectors.
//!
//! Each test starts with a reset and a program at RESET_PC. A test does not
//! see the replies that an earlier test left in the memory pipe. This way one
//! failure does not cause a chain of failures.

// A testbench compares values of different widths on purpose.
/* verilator lint_off WIDTHEXPAND */
/* verilator lint_off WIDTHTRUNC */

module tb_fetch #(
    parameter int LATENCY = 1
  );
  import tb_util_pkg::*;

  localparam logic [63:0] RESET_PC = 64'h40;

  logic clk = 1'b0;
  logic rst = 1'b1;
  logic redirect_valid = 1'b0;
  logic [63:0] redirect_pc = '0;
  logic insn_ready = 1'b1;
  wire imem_rvalid;
  wire [95:0] imem_rdata;
  wire insn_valid;
  wire imem_req;
  wire [63:0] imem_addr;
  wire [63:0] insn_pc;
  wire [95:0] insn_bytes;
  wire [3:0] insn_len;

  vea_fetch #(.RESET_PC(RESET_PC)) dut (.*);

  //! Fetch asks for bytes in reset, and before the first clock edge its pc is not valid. A real memory ignores requests in reset.
  tb_imem #(.LATENCY(LATENCY)) imem (
            .clk(clk),
            .req(imem_req && !rst),
            .addr(imem_addr),
            .rvalid(imem_rvalid),
            .rdata(imem_rdata)
          );

  always #5 clk <= ~clk;

  //! The test drives inputs and reads outputs 1 time unit after the edge. This way it never races with the clock.
  task automatic tick();
    @(posedge clk);
    #1;
  endtask

  //! The reset is longer than the memory latency. Replies of an old request arrive during the reset, and fetch ignores them.
  task automatic reset_dut();
    rst = 1'b1;
    repeat (LATENCY + 2) tick();
    rst = 1'b0;
    #1;
  endtask

  task automatic redirect(input logic [63:0] pc);
    redirect_valid = 1'b1;
    redirect_pc = pc;
    tick();
    redirect_valid = 1'b0;
  endtask

  //! A hang would give a wrong pass. The limit makes it a failure.
  task automatic wait_valid();
    int n = 0;
    while (!insn_valid)
    begin
      tick();
      n++;
      if (n > 50)
        $fatal(1, "tb_fetch: no instruction after 50 cycles");
    end
  endtask

  task automatic wait_req(input string name, input logic [63:0] addr);
    int n = 0;
    while (!imem_req)
    begin
      tick();
      n++;
      if (n > 50)
        $fatal(1, "tb_fetch: no memory request after 50 cycles");
    end
    check_eq({name, ": request address"}, imem_addr, addr);
  endtask

  //! Only the first "n" bytes belong to the instruction. Fetch does not need to clear the other bytes.
  task automatic expect_insn(input string name, input logic [63:0] pc, input logic [95:0] lit, input int n);
    wait_valid();
    check_eq({name, ": pc"}, insn_pc, pc);
    check_eq({name, ": length"}, insn_len, n);
    check_eq({name, ": bytes"}, insn_bytes >> (96 - 8 * n), lit);
  endtask

  //! The instruction leaves fetch on this edge, because insn_ready is 1.
  task automatic consume();
    tick();
  endtask

  //! An instruction of "n" bytes ends at pc + n. The next instruction starts at the next multiple of 4.
  function automatic logic [63:0] next_pc(input logic [63:0] pc, input int n);
    return ((pc + n + 3) / 4) * 4;
  endfunction

  task automatic expect_idle(input string name, input int cycles);
    for (int i = 0; i < cycles; i++)
    begin
      check_eq({name, ": no request"}, imem_req, 1'b0);
      check_eq({name, ": no instruction"}, insn_valid, 1'b0);
      tick();
    end
  endtask

  //! Program from the assembler vectors. The last instruction is a halt.
  task automatic test_program_and_halt();
    imem.put(64'h40, 40'h4057010210, 5);
    imem.put(64'h48, 32'h01410105, 4);
    imem.put(64'h4C, 16'h0020, 2);
    imem.put(64'h50, 16'hFF20, 2);

    reset_dut();
    wait_req("first fetch", RESET_PC);
    expect_insn("ld.w", 64'h40, 40'h4057010210, 5);
    consume();
    wait_req("after a 5 byte instruction", 64'h48);
    expect_insn("mov", 64'h48, 32'h01410105, 4);
    consume();
    wait_req("after a 4 byte instruction", 64'h4C);
    expect_insn("nop", 64'h4C, 16'h0020, 2);
    consume();
    wait_req("after a 2 byte instruction", 64'h50);
    expect_insn("halt", 64'h50, 16'hFF20, 2);
    consume();
    expect_idle("after halt", 6);
  endtask

  //! A redirect must start fetch again after a halt. This test needs the halt state of the test before it.
  task automatic test_redirect_from_halt();
    redirect(RESET_PC);
    wait_req("restart after halt", RESET_PC);
    expect_insn("ld.w again", 64'h40, 40'h4057010210, 5);
  endtask

  //! Every legal length, 2 to 12. Each instruction sits where the previous one ends, rounded up to 4.
  task automatic test_next_pc_for_every_length();
    logic [63:0] pc = RESET_PC;
    logic [63:0] starts [2:12];
    for (int n = 2; n <= 12; n++)
    begin
      starts[n] = pc;
      imem.put(int'(pc), {8'h00, 4'(n), 4'h0}, 2);
      pc = next_pc(pc, n);
    end
    reset_dut();
    for (int n = 2; n <= 12; n++)
    begin
      wait_req($sformatf("length %0d", n), starts[n]);
      wait_valid();
      check_eq($sformatf("length %0d: pc", n), insn_pc, starts[n]);
      check_eq($sformatf("length %0d: length", n), insn_len, n);
      consume();
    end
    wait_req("after length 12", pc);
  endtask

  //! The instruction must stay the same while decode is not ready. Fetch must not ask for more bytes.
  task automatic test_backpressure();
    imem.put(64'h40, 40'h4057010210, 5);
    imem.put(64'h48, 32'h01410105, 4);
    reset_dut();
    wait_valid();
    insn_ready = 1'b0;
    for (int i = 0; i < 5; i++)
    begin
      expect_insn("held", 64'h40, 40'h4057010210, 5);
      check_eq("held: no request", imem_req, 1'b0);
      tick();
    end
    insn_ready = 1'b1;
    consume();
    wait_req("after the stall", 64'h48);
    expect_insn("next after the stall", 64'h48, 32'h01410105, 4);
    consume();
  endtask

  //! The redirect has priority over insn_ready. The pc must not take pc_next.
  task automatic test_redirect_while_sending();
    imem.put(64'h40, 16'h0020, 2);
    imem.put(64'h280, 40'h4057010210, 5);
    reset_dut();
    wait_valid();
    insn_ready = 1'b1;
    redirect(64'h280);
    check_eq("redirect from send: no stale instruction", insn_valid, 1'b0);
    wait_req("redirect from send", 64'h280);
    expect_insn("redirect from send", 64'h280, 40'h4057010210, 5);
    consume();
  endtask

  //! The request of the old pc leaves in the cycle of the redirect. Its reply must not become the instruction.
  //! With memory latency 2 or more, the reply arrives while fetch waits for the new address.
  task automatic test_redirect_while_asking();
    imem.put(64'h40, 16'h0020, 2);
    imem.put(64'h300, 32'h01410105, 4);
    reset_dut();
    wait_req("ask", RESET_PC);
    redirect(64'h300);
    wait_req("redirect while asking", 64'h300);
    expect_insn("redirect while asking", 64'h300, 32'h01410105, 4);
    consume();
  endtask

  //! The redirect can come while memory has not replied. The late reply for the old address must not become the instruction.
  task automatic test_redirect_while_waiting();
    imem.put(64'h40, 16'h0020, 2);
    imem.put(64'h380, 32'h01410105, 4);
    reset_dut();
    wait_req("wait", RESET_PC);
    tick();
    redirect(64'h380);
    wait_valid();
    check_eq("redirect while waiting: pc", insn_pc, 64'h380);
    check_eq("redirect while waiting: bytes", insn_bytes >> 64, 32'h01410105);
    consume();
  endtask

  //! A second redirect can come while the unit still waits for the first old reply. Fetch must not ask for the address of the first redirect.
  task automatic test_double_redirect();
    imem.put(64'h40, 16'h0020, 2);
    imem.put(64'h300, 32'h01410105, 4);
    imem.put(64'h380, 40'h4057010210, 5);
    reset_dut();
    wait_req("ask", RESET_PC);
    redirect(64'h300);
    redirect(64'h380);
    expect_insn("double redirect", 64'h380, 40'h4057010210, 5);
    consume();
  endtask

  //! The pc must go back to RESET_PC, and not stay at the address of the instruction in progress.
  task automatic test_reset_in_the_middle();
    imem.put(64'h40, 16'h0020, 2);
    imem.put(64'h44, 16'h0020, 2);
    reset_dut();
    expect_insn("before reset", 64'h40, 16'h0020, 2);
    consume();
    expect_insn("before reset, second", 64'h44, 16'h0020, 2);
    reset_dut();
    wait_req("reset in the middle", RESET_PC);
    expect_insn("after reset", 64'h40, 16'h0020, 2);
  endtask

  initial
  begin
    $display("tb_fetch: memory latency %0d", LATENCY);
    test_program_and_halt();
    test_redirect_from_halt();
    test_next_pc_for_every_length();
    test_backpressure();
    test_redirect_while_sending();
    test_redirect_while_asking();
    test_redirect_while_waiting();
    test_double_redirect();
    test_reset_in_the_middle();
    report("tb_fetch");
    $finish;
  end

  initial
  begin
    #1000000;
    $fatal(1, "tb_fetch: timeout");
  end

endmodule
