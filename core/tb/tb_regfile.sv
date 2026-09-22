//! Testbench for vea_regfile.

module tb_regfile #(
  parameter int SEED = 0
);
  localparam int NUM_REGS = 32;

  logic clk = 1'b0;
  always #5 clk <= ~clk;

  logic        wr_en   = 1'b0;
  logic [4:0]  wr_addr = '0;
  logic [63:0] wr_data = '0;
  logic [4:0]  raddr_a = '0;
  logic [4:0]  raddr_b = '0;
  logic [63:0] rdata_a;
  logic [63:0] rdata_b;

  vea_regfile dut (.*);

  int checks = 0;
  int errors = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_regfile: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  // The write reaches the array at the next clock edge.
  task automatic write_reg(input logic [4:0] addr, input logic [63:0] data);
    wr_en   = 1'b1;
    wr_addr = addr;
    wr_data = data;
    tick();
    wr_en = 1'b0;
  endtask

  // The read ports have no clock, so the delay lets the outputs settle.
  task automatic read_regs(input logic [4:0] a, input logic [4:0] b);
    raddr_a = a;
    raddr_b = b;
    #1;
  endtask

  // Each n gives a different value, so a register that reads the value of another one is
  // an error. The value is never zero, so it also finds a register that is fixed at zero.
  function automatic logic [63:0] pattern(input int n);
    pattern = 64'h9E37_79B9_7F4A_7C15 * (64'(n) + 64'd1);
  endfunction

  // Half of the addresses are in the range 0 to 3, so a write and a read of the same
  // register happen often.
  function automatic logic [4:0] rand_addr();
    if ($urandom_range(0, 1) == 0) return 5'($urandom_range(0, 3));
    rand_addr = 5'($urandom_range(0, 31));
  endfunction

  // The simulator starts with all registers at zero. This test must run before the first
  // write.
  task automatic test_power_up();
    for (int i = 0; i < NUM_REGS; i++) begin
      read_regs(5'(i), 5'(31 - i));
      check64(rdata_a, 64'h0, $sformatf("power-up r%0d on port a", i));
      check64(rdata_b, 64'h0, $sformatf("power-up r%0d on port b", i));
    end
  endtask

  // A model with the same behavior finds a write that goes to the wrong register, a
  // write that comes from a cycle without wr_en, and a wrong read of the new value. The
  // model must start in the power-up state, so this test runs right after the power-up
  // test.
  task automatic test_random(input int cycles);
    logic [63:0] model [NUM_REGS];
    logic [63:0] want_a, want_b;

    for (int i = 0; i < NUM_REGS; i++) model[i] = '0;

    for (int n = 0; n < cycles; n++) begin
      wr_en   = ($urandom_range(0, 3) != 0);
      wr_addr = rand_addr();
      wr_data = {$urandom, $urandom};
      raddr_a = rand_addr();
      raddr_b = rand_addr();
      #1;

      want_a = model[raddr_a];
      want_b = model[raddr_b];
      check64(rdata_a, want_a, $sformatf("random cycle %0d, port a, r%0d", n, raddr_a));
      check64(rdata_b, want_b, $sformatf("random cycle %0d, port b, r%0d", n, raddr_b));

      if (wr_en) model[wr_addr] = wr_data;
      tick();
    end
    wr_en = 1'b0;
  endtask

  // Both ports must read every register, and they must read two different registers in
  // the same cycle. Vea has no zero register, so r0 must hold a value.
  task automatic test_all_registers();
    int j;

    for (int i = 0; i < NUM_REGS; i++) write_reg(5'(i), pattern(i));

    for (int i = 0; i < NUM_REGS; i++) begin
      // The factor 5 is odd, so port b reads each register once.
      j = (i * 5 + 3) % NUM_REGS;
      read_regs(5'(i), 5'(j));
      check64(rdata_a, pattern(i), $sformatf("r%0d on port a", i));
      check64(rdata_b, pattern(j), $sformatf("r%0d on port b", j));
    end
  endtask

  // A write without wr_en must not change the register. It must also not reach a read of
  // the same register, because the write does not happen. This test needs the values from
  // test_all_registers.
  task automatic test_write_enable();
    wr_en   = 1'b0;
    wr_addr = 5'd6;
    wr_data = ~pattern(6);
    tick(2);
    read_regs(5'd6, 5'd7);
    check64(rdata_a, pattern(6), "r6 after a write with wr_en low");
    check64(rdata_b, pattern(7), "r7 after a write with wr_en low");
  endtask

  // A read of the register the write port is writing this same cycle must still return
  // the old value, on each port: there is no same-cycle bypass. The write must still
  // reach the array at the next edge. This test needs the values from test_all_registers.
  task automatic test_no_bypass();
    logic [63:0] fresh;

    fresh   = ~pattern(3);
    wr_en   = 1'b1;
    wr_addr = 5'd3;
    wr_data = fresh;

    read_regs(5'd3, 5'd4);
    check64(rdata_a, pattern(3),  "no bypass on port a");
    check64(rdata_b, pattern(4),  "port b read of r4 during a write to r3");

    read_regs(5'd4, 5'd3);
    check64(rdata_a, pattern(4),  "port a read of r4 during a write to r3");
    check64(rdata_b, pattern(3),  "no bypass on port b");

    read_regs(5'd3, 5'd3);
    check64(rdata_a, pattern(3),  "no bypass on port a, same register on both ports");
    check64(rdata_b, pattern(3),  "no bypass on port b, same register on both ports");

    // The write must still reach the array.
    tick();
    wr_en = 1'b0;
    read_regs(5'd3, 5'd4);
    check64(rdata_a, fresh,       "r3 after the write");
    check64(rdata_b, pattern(4),  "r4 after the write to r3");
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_regfile: seed %0d", SEED);
    $display("tb_regfile: test_power_up");
    test_power_up();
    $display("tb_regfile: test_random");
    test_random(50000);
    $display("tb_regfile: test_all_registers");
    test_all_registers();
    $display("tb_regfile: test_write_enable");
    test_write_enable();
    $display("tb_regfile: test_no_bypass");
    test_no_bypass();

    $display("tb_regfile: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_regfile failed");
    $finish;
  end

  initial begin
    #5000000;
    $fatal(1, "tb_regfile watchdog");
  end
endmodule
