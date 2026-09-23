//! Testbench for vea_adder. See ../alu.sv for the module.
//!
//! One instance builds a + b and one builds a - b. Both share one operand pair.

module tb_adder #(
  parameter int SEED = 0
);
  logic clk;

  initial begin
    clk = 1'b0;
    forever #5 clk = ~clk;
  end

  logic [63:0] a = '0;
  logic [63:0] b = '0;

  logic [63:0] add_result;
  logic [63:0] sub_result;

  vea_adder #(.SUB(1'b0)) u_add (.clk(clk), .a(a), .b(b), .result(add_result));
  vea_adder #(.SUB(1'b1)) u_sub (.clk(clk), .a(a), .b(b), .result(sub_result));

  int checks = 0;
  int errors = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_adder: %s: got %h, expected %h", what, got, want);
    end
  endtask

  // ---- One operand pair through every instance -------------------------------------------

  // Drives one pair for one clock edge. Then it puts random values on a and b. The result
  // must come from the registers only, so a design with no register cannot pass. The pair
  // changes on every edge, so back-to-back operands also get checked.
  task automatic step(input logic [63:0] av, input logic [63:0] bv, input string name);
    a = av;
    b = bv;
    @(posedge clk);
    #1;
    a = {$urandom, $urandom};
    b = {$urandom, $urandom};
    #1;

    check64(add_result, av + bv, {name, ": add"});
    check64(sub_result, av - bv, {name, ": sub"});
  endtask

  // ---- Directed operand pairs ----------------------------------------------------------

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
        step(dv[i], dv[j], $sformatf("directed a=%0d b=%0d", i, j));
      end
    end
  endtask

  // A carry that starts at bit 0 and stops at bit k crosses every bit below k. The borrow
  // of a subtract does the same.
  task automatic test_ripple();
    logic [63:0] low_ones;
    logic [63:0] one_bit;

    for (int k = 0; k < 64; k++) begin
      one_bit  = 64'd1 << k;
      low_ones = one_bit - 64'd1;
      step(low_ones, 64'd1,  $sformatf("ripple ones below bit %0d, + 1", k));
      step(one_bit,  64'd1,  $sformatf("ripple borrow to bit %0d, - 1", k));
      step(64'd1,    low_ones, $sformatf("ripple, operands swapped, bit %0d", k));
      step(low_ones, one_bit, $sformatf("ripple, no carry out of bit %0d", k));
    end
  endtask

  // ---- Random operand pairs ------------------------------------------------------------

  task automatic test_random(input int n);
    for (int k = 0; k < n; k++) begin
      step({$urandom, $urandom}, {$urandom, $urandom}, $sformatf("random %0d", k));
    end
  endtask

  // b is ~a plus a small offset. The sum is then all ones, or close to it. For a - b the
  // same offsets on b put a and b nearly equal.
  task automatic test_near_complement(input int n);
    logic [63:0] av;

    for (int k = 0; k < n; k++) begin
      av = {$urandom, $urandom};
      step(av, ~av,                        $sformatf("complement %0d", k));
      step(av, ~av + 64'd1,                $sformatf("complement + 1 %0d", k));
      step(av, ~av - 64'd1,                $sformatf("complement - 1 %0d", k));
      step(av, av,                         $sformatf("equal %0d", k));
      step(av, av + 64'($urandom_range(1, 3)), $sformatf("near equal %0d", k));
    end
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_adder: seed %0d", SEED);
    init_directed();
    $display("tb_adder: test_directed");
    test_directed();
    $display("tb_adder: test_ripple");
    test_ripple();
    $display("tb_adder: test_random");
    test_random(5000);
    $display("tb_adder: test_near_complement");
    test_near_complement(1000);

    $display("tb_adder: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_adder failed");
    $finish;
  end

  initial begin
    #50000000;
    $fatal(1, "tb_adder watchdog");
  end
endmodule
