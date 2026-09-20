//! Testbench for vea_alu.
//!
//! MUL and DIV stop the simulation until the real unit exists. The test does not run them.

// A testbench compares values of different widths on purpose.
/* verilator lint_off WIDTHEXPAND */
/* verilator lint_off WIDTHTRUNC */

module tb_alu;
  import tb_util_pkg::*;

  logic clk = 1'b0;
  logic [63:0] in_a = '0;
  logic [63:0] in_b = '0;
  logic [7:0] operation = '0;
  wire [63:0] result;

  vea_alu dut (.*);

  always #5 clk <= ~clk;

  //! The result is a register. The test reads it just after the clock edge.
  task automatic run_op(input string name, input logic [7:0] op, input logic [63:0] a, input logic [63:0] b,
                        input logic [63:0] expected);
    operation = op;
    in_a = a;
    in_b = b;
    @(posedge clk);
    #1;
    check_eq(name, result, expected);
  endtask

  task automatic test_arithmetic();
    run_op("add", `ALU_ADD, 64'd5, 64'd7, 64'd12);
    run_op("add wraps", `ALU_ADD, 64'hFFFF_FFFF_FFFF_FFFF, 64'd1, 64'd0);
    run_op("sub", `ALU_SUB, 64'd7, 64'd5, 64'd2);
    run_op("sub borrows", `ALU_SUB, 64'd0, 64'd1, 64'hFFFF_FFFF_FFFF_FFFF);
  endtask

  task automatic test_logic();
    run_op("and", `ALU_AND, 64'hFF00_FF00_FF00_FF00, 64'h0FF0_0FF0_0FF0_0FF0, 64'h0F00_0F00_0F00_0F00);
    run_op("or", `ALU_OR, 64'hFF00_0000_0000_0000, 64'h0000_0000_0000_00FF, 64'hFF00_0000_0000_00FF);
    run_op("xor", `ALU_XOR, 64'hFFFF_0000_FFFF_0000, 64'hFF00_FF00_FF00_FF00, 64'h00FF_FF00_00FF_FF00);
    run_op("not", `ALU_NOT, 64'h0000_0000_0000_00FF, 64'hDEAD_BEEF_DEAD_BEEF, 64'hFFFF_FFFF_FFFF_FF00);
  endtask

  task automatic test_shifts();
    run_op("shl", `ALU_SHL, 64'd1, 64'd4, 64'd16);
    run_op("shl by 63", `ALU_SHL, 64'd1, 64'd63, 64'h8000_0000_0000_0000);
    run_op("shr fills zero", `ALU_SHR, 64'h8000_0000_0000_0000, 64'd4, 64'h0800_0000_0000_0000);
    run_op("sar keeps the sign", `ALU_SAR, 64'h8000_0000_0000_0000, 64'd4, 64'hF800_0000_0000_0000);
    run_op("sar of a positive value", `ALU_SAR, 64'h4000_0000_0000_0000, 64'd4, 64'h0400_0000_0000_0000);
    //! The shift amount has 6 bits, so 65 shifts by 1.
    run_op("shl uses the low 6 bits", `ALU_SHL, 64'd1, 64'd65, 64'd2);
    run_op("shr uses the low 6 bits", `ALU_SHR, 64'd4, 64'd65, 64'd2);
    run_op("shift by 0", `ALU_SHL, 64'h1234, 64'd0, 64'h1234);
  endtask

  //! A registered result must not change before the clock edge.
  task automatic test_registered_result();
    run_op("setup", `ALU_ADD, 64'd1, 64'd1, 64'd2);
    operation = `ALU_ADD;
    in_a = 64'd10;
    in_b = 64'd10;
    #1;
    check_eq("result holds before the edge", result, 64'd2);
    @(posedge clk);
    #1;
    check_eq("result updates after the edge", result, 64'd20);
  endtask

  //! An unknown operation must keep the old result, and not give an unknown value.
  task automatic test_unknown_operation();
    run_op("setup", `ALU_ADD, 64'd3, 64'd4, 64'd7);
    run_op("op 0x0F keeps the result", 8'h0F, 64'd100, 64'd200, 64'd7);
    run_op("op 0x10 keeps the result", 8'h10, 64'd100, 64'd200, 64'd7);
  endtask

  initial
  begin
    test_arithmetic();
    test_logic();
    test_shifts();
    test_registered_result();
    test_unknown_operation();
    report("tb_alu");
    $finish;
  end

  initial
  begin
    #100000;
    $fatal(1, "tb_alu: timeout");
  end

endmodule
