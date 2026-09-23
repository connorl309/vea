//! Testbench for vea_mul. See ../mul.sv for the module.

module tb_mul #(
  parameter int SEED = 0
);
  logic clk   = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  logic        start = 1'b0;
  logic [63:0] a     = '0;
  logic [63:0] b     = '0;
  logic        valid;
  logic [63:0] result;

  vea_mul dut (.*);

  int errors = 0;
  int checks = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_mul: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  task automatic do_reset();
    rst_n = 1'b0;
    start = 1'b0;
    tick(3);
    rst_n = 1'b1;
  endtask

  task automatic test_reset();
    do_reset();
    check64(64'(valid), 64'b0, "valid after reset");
  endtask

  // Drives one multiply and waits for valid. Also checks valid drops one cycle later, so
  // a stuck-high valid cannot pass the next multiply by luck.
  task automatic do_mul(input logic [63:0] av, input logic [63:0] bv, input string name);
    a     = av;
    b     = bv;
    start = 1'b1;
    tick();
    start = 1'b0;
    while (!valid) tick();
    check64(result, av * bv, {name, ": result"});
    tick();
    check64(64'(valid), 64'b0, {name, ": valid drops one cycle later"});
  endtask

  // ---- Directed operand pairs, reused across tests ---------------------------------------

  localparam int N_DIRECTED = 8;
  logic [63:0] dv [N_DIRECTED];

  function automatic void init_directed();
    dv[0] = 64'h0;
    dv[1] = 64'h1;
    dv[2] = 64'hFFFF_FFFF_FFFF_FFFF;
    dv[3] = 64'h8000_0000_0000_0000; // INT64_MIN
    dv[4] = 64'h7FFF_FFFF_FFFF_FFFF; // INT64_MAX
    dv[5] = 64'h5555_5555_5555_5555;
    dv[6] = 64'hAAAA_AAAA_AAAA_AAAA;
    dv[7] = 64'hDEAD_BEEF_CAFE_F00D;
  endfunction

  task automatic test_directed();
    for (int i = 0; i < N_DIRECTED; i++) begin
      for (int j = 0; j < N_DIRECTED; j++) begin
        do_mul(dv[i], dv[j], $sformatf("directed a=%0d b=%0d", i, j));
      end
    end
  endtask

  task automatic test_random(input int n);
    logic [63:0] av, bv;
    for (int k = 0; k < n; k++) begin
      av = {$urandom, $urandom};
      bv = {$urandom, $urandom};
      do_mul(av, bv, $sformatf("random %0d", k));
    end
  endtask

  // Execute only ever holds one multiply at a time: it will not pulse start again before
  // valid. This runs two multiplies back to back to check the module is ready right away.
  task automatic test_back_to_back();
    do_mul(64'd6, 64'd7, "back to back 1");
    do_mul(64'd1000, 64'd1000, "back to back 2");
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_mul: seed %0d", SEED);
    init_directed();
    $display("tb_mul: test_reset");
    test_reset();
    $display("tb_mul: test_directed");
    test_directed();
    $display("tb_mul: test_back_to_back");
    test_back_to_back();
    $display("tb_mul: test_random");
    test_random(2000);

    $display("tb_mul: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_mul failed");
    $finish;
  end

  initial begin
    #2000000;
    $fatal(1, "tb_mul watchdog");
  end
endmodule
