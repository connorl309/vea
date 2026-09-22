//! Combinational arithmetic logic unit for Vea.
//!
//! The module has no clock. The stage that uses it holds the result register.
//!
//! NOT reads only operand A. Unary operands place their source in A.

module vea_alu
(
  input  logic [63:0] a,
  input  logic [63:0] b,
  input  logic [3:0]  op,
  output logic [63:0] result,
  output logic [3:0] flags,
  //! High only for CMP and CMP_S. Other operations must not change the condition codes.
  output logic        flags_valid,
  //! High for MUL, DIV, and any undefined operation. MUL and DIV have no silicon yet, so
  //! the core must fault instead of it writing a false result.
  output logic        unsupported
);

  // The values 0 to A are the low nibble of the opcode, so Decode can pass them
  // through. The opcodes of CMP and CMP_S are in a different group, so they use
  // the next free values.
  typedef enum logic [3:0] {
    ALU_ADD   = 4'h0,
    ALU_SUB   = 4'h1,
    ALU_AND   = 4'h2,
    ALU_OR    = 4'h3,
    ALU_NOT   = 4'h4,
    ALU_XOR   = 4'h5,
    ALU_SHL   = 4'h6,
    ALU_SHR   = 4'h7,
    ALU_SAR   = 4'h8,
    ALU_MUL   = 4'h9,
    ALU_DIV   = 4'hA,
    ALU_CMP   = 4'hB,
    ALU_CMP_S = 4'hC
  } alu_op_t;

  // The bit order is the same as the condition codes in the simulator.
  typedef struct packed {
    logic zero;
    logic neg;
    logic carry;
    logic overflow;
  } flags_t;

  flags_t      flags_next;
  logic [63:0] diff;
  logic [5:0]  shamt;
  logic        sub_overflow;

  // SUB, CMP, and CMP_S share one subtractor.
  assign diff  = a - b;
  // The simulator uses only the low six bits of B as the shift amount.
  assign shamt = b[5:0];
  assign sub_overflow = (a[63] != b[63]) && (diff[63] != a[63]);
  assign flags = flags_next;

  always_comb begin
    result      = 64'b0;
    flags_next  = '0;
    flags_valid = 1'b0;
    unsupported = 1'b0;

    case (op)
      ALU_ADD: result = a + b;
      ALU_SUB: result = diff;
      ALU_AND: result = a & b;
      ALU_OR:  result = a | b;
      ALU_NOT: result = ~a;
      ALU_XOR: result = a ^ b;
      ALU_SHL: result = a << shamt;
      ALU_SHR: result = a >> shamt;
      ALU_SAR: result = $signed(a) >>> shamt;

      // Carry means no borrow, as in the simulator.
      ALU_CMP: begin
        flags_valid    = 1'b1;
        flags_next.zero     = (a == b);
        flags_next.neg      = (a < b);
        flags_next.carry    = (a >= b);
        flags_next.overflow = 1'b0;
      end

      ALU_CMP_S: begin
        flags_valid    = 1'b1;
        flags_next.zero     = (a == b);
        flags_next.neg      = diff[63];
        flags_next.carry    = (a >= b);
        flags_next.overflow = sub_overflow;
      end

      // MUL and DIV are legal opcodes with no silicon. Trip an assertion so a simulation
      // flags them instead of silently returning zero.
      ALU_MUL: begin
        unsupported = 1'b1;
        assert (1'b0) else $display("vea_alu: MUL has no silicon (op %h)", op);
      end

      ALU_DIV: begin
        unsupported = 1'b1;
        assert (1'b0) else $display("vea_alu: DIV has no silicon (op %h)", op);
      end

      default: unsupported = 1'b1;
    endcase
  end

endmodule
