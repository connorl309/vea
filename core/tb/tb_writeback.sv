//! Testbench for vea_writeback. See ../writeback.sv for the module.

module tb_writeback #(
  parameter int SEED = 0
);
  logic clk   = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  logic        wb_valid          = 1'b0;
  logic [4:0]  wb_rd             = '0;
  logic [63:0] wb_data           = '0;
  logic        wb_redirect_valid = 1'b0;
  logic [63:2] wb_redirect_pc    = '0;

  logic        rf_wr_en;
  logic [4:0]  rf_wr_addr;
  logic [63:0] rf_wr_data;
  logic        redirect_valid;
  logic [63:2] redirect_pc;

  vea_writeback dut (.*);

  int errors = 0;
  int checks = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_writeback: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  task automatic do_reset();
    rst_n              = 1'b0;
    wb_valid           = 1'b0;
    wb_redirect_valid  = 1'b0;
    tick(3);
    rst_n = 1'b1;
  endtask

  task automatic test_reset();
    do_reset();
    check64(64'(rf_wr_en),       64'b0, "rf_wr_en after reset");
    check64(64'(redirect_valid), 64'b0, "redirect_valid after reset");
  endtask

  // A single write pulse must land exactly one cycle later, and must not repeat. The
  // redirect path must stay idle throughout, since this pulse does not touch it.
  task automatic test_write_pulse();
    wb_valid = 1'b1;
    wb_rd    = 5'd13;
    wb_data  = 64'hFEED_FACE_1234_5678;
    tick();
    wb_valid = 1'b0;
    check64(64'(rf_wr_en),   64'b1,                   "write pulse: rf_wr_en one cycle later");
    check64(64'(rf_wr_addr), 64'd13,                  "write pulse: rf_wr_addr one cycle later");
    check64(rf_wr_data,      64'hFEED_FACE_1234_5678, "write pulse: rf_wr_data one cycle later");
    check64(64'(redirect_valid), 64'b0, "write pulse: redirect_valid stays low");
    tick();
    check64(64'(rf_wr_en), 64'b0, "write pulse: rf_wr_en drops one cycle after the pulse");
  endtask

  // The redirect path, mirrored against test_write_pulse. The write path must stay idle.
  task automatic test_redirect_pulse();
    wb_redirect_valid = 1'b1;
    wb_redirect_pc    = 62'h1_0000;
    tick();
    wb_redirect_valid = 1'b0;
    check64(64'(redirect_valid), 64'b1,     "redirect pulse: redirect_valid one cycle later");
    check64(64'(redirect_pc),    64'h1_0000, "redirect pulse: redirect_pc one cycle later");
    check64(64'(rf_wr_en), 64'b0, "redirect pulse: rf_wr_en stays low");
    tick();
    check64(64'(redirect_valid), 64'b0, "redirect pulse: redirect_valid drops one cycle after the pulse");
  endtask

  // A random stream of write and redirect cycles, driven independently of each other.
  // rf_wr_addr/rf_wr_data and redirect_pc must hold their last value through a cycle
  // where the matching valid signal is low, since nothing downstream reads them then.
  task automatic test_random(input int cycles);
    logic [4:0]  last_rd;
    logic [63:0] last_data;
    logic [63:2] last_target;

    // rf_wr_addr, rf_wr_data and redirect_pc have no reset, so this test must not assume
    // they start at zero. A priming write and redirect give the model a known start.
    wb_valid          = 1'b1;
    wb_rd             = 5'd0;
    wb_data           = 64'd0;
    wb_redirect_valid = 1'b1;
    wb_redirect_pc    = 62'd0;
    tick();
    last_rd     = wb_rd;
    last_data   = wb_data;
    last_target = wb_redirect_pc;

    for (int n = 0; n < cycles; n++) begin
      wb_valid          = ($urandom_range(0, 3) != 0);
      wb_rd             = 5'($urandom_range(0, 31));
      wb_data           = {$urandom, $urandom};
      wb_redirect_valid = ($urandom_range(0, 3) != 0);
      wb_redirect_pc    = 62'($urandom);
      tick();

      check64(64'(rf_wr_en), 64'(wb_valid), $sformatf("random cycle %0d: rf_wr_en", n));
      if (wb_valid) begin
        check64(64'(rf_wr_addr), 64'(wb_rd), $sformatf("random cycle %0d: rf_wr_addr", n));
        check64(rf_wr_data,      wb_data,    $sformatf("random cycle %0d: rf_wr_data", n));
        last_rd   = wb_rd;
        last_data = wb_data;
      end else begin
        check64(64'(rf_wr_addr), 64'(last_rd), $sformatf("random cycle %0d: rf_wr_addr holds", n));
        check64(rf_wr_data,      last_data,    $sformatf("random cycle %0d: rf_wr_data holds", n));
      end

      check64(64'(redirect_valid), 64'(wb_redirect_valid),
              $sformatf("random cycle %0d: redirect_valid", n));
      if (wb_redirect_valid) begin
        check64(64'(redirect_pc), 64'(wb_redirect_pc), $sformatf("random cycle %0d: redirect_pc", n));
        last_target = wb_redirect_pc;
      end else begin
        check64(64'(redirect_pc), 64'(last_target), $sformatf("random cycle %0d: redirect_pc holds", n));
      end
    end
    wb_valid          = 1'b0;
    wb_redirect_valid = 1'b0;
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_writeback: seed %0d", SEED);
    $display("tb_writeback: test_reset");
    test_reset();
    $display("tb_writeback: test_write_pulse");
    test_write_pulse();
    $display("tb_writeback: test_redirect_pulse");
    test_redirect_pulse();
    $display("tb_writeback: test_random");
    test_random(20000);

    $display("tb_writeback: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_writeback failed");
    $finish;
  end

  initial begin
    #2000000;
    $fatal(1, "tb_writeback watchdog");
  end
endmodule
