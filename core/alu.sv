//! Arithmetic logic unit for Vea.
//!
//! ADD and SUB have registers, inside vea_adder. Their result is valid one clock edge
//! after a and b. All other results are combinational. The stage that uses the ALU holds
//! op, a and b until an ADD or SUB result is valid. The late output tells it when to wait.
//!
//! NOT reads only operand A. Unary operands place their source in A.

module vea_alu
(
  input  logic        clk,
  input  logic [63:0] a,
  input  logic [63:0] b,
  input  logic [3:0]  op,
  output logic [63:0] result,
  //! The result of ADD, whatever op holds. It comes straight from the adder register, one
  //! clock edge after a and b. A load address, a store address and a branch target always
  //! use ADD. A stage that reads this port has no op mux and no shifter in its path.
  output logic [63:0] add_result,
  output logic [3:0] flags,
  //! High only for CMP and CMP_S. Other operations must not change the condition codes.
  output logic        flags_valid,
  //! High for DIV, and any undefined operation. DIV has no silicon yet. The core must
  //! fault instead of writing a false result.
  //!
  //! MUL has its own module now, vea_mul. This ALU takes no part in it.
  output logic        unsupported,
  //! High for ADD and SUB. The stage must wait one clock edge for their result.
  output logic        late
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
  logic        diff_sign;
  logic [5:0]  shamt;
  logic        sub_overflow;

  logic [63:0] sub_result;

  // ADD and SUB each have their own adder.
  vea_adder #(.SUB(1'b0)) u_add (.clk(clk), .a(a), .b(b), .result(add_result));
  vea_adder #(.SUB(1'b1)) u_sub (.clk(clk), .a(a), .b(b), .result(sub_result));

  // CMP_S needs the sign of a - b in the same cycle as its operands. u_sub is one edge
  // late, so CMP_S has its own subtractor. It needs only the sign.
  assign diff_sign = 1'((a - b) >> 63);
  // The simulator uses only the low six bits of B as the shift amount.
  assign shamt = b[5:0];
  assign sub_overflow = (a[63] != b[63]) && (diff_sign != a[63]);
  assign flags = flags_next;

  always_comb begin
    result      = 64'b0;
    flags_next  = '0;
    flags_valid = 1'b0;
    unsupported = 1'b0;
    late        = 1'b0;

    case (op)
      ALU_ADD: begin
        result = add_result;
        late   = 1'b1;
      end

      ALU_SUB: begin
        result = sub_result;
        late   = 1'b1;
      end

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
        flags_next.neg      = diff_sign;
        flags_next.carry    = (a >= b);
        flags_next.overflow = sub_overflow;
      end

      // vea_mul computes MUL now, not this case. This case only keeps MUL out of the
      // unsupported default below.
      ALU_MUL: ;

      // TODO: Come up with a not-shitty division idea here.
      // DIV can reuse the multi-cycle path vea_mul now gives MUL.
      ALU_DIV: begin
        unsupported = 1'b1;
        /*assert (1'b0); $display("vea_alu: DIV has no silicon (op %h)", op); */
      end

      default: unsupported = 1'b1;
    endcase
  end

endmodule


//! Registered 64-bit adder/subtractor. SUB = 0 gives a + b. SUB = 1 gives a - b.
//!
//! In Execute, a 64-bit carry chain follows the forward mux and feeds the write-back mux.
//! This path impacts the critical path. I believe that this is the best design for
//! the ECP-5 architecture.

module vea_adder #(
  //! High builds a - b. Low builds a + b. ADD and SUB each get one instance. Then no mux
  //! picks between a + b and a - b in front of the chain.
  parameter logic SUB = 1'b0
) (
  input  logic        clk,
  input  logic [63:0] a,
  input  logic [63:0] b,
  //! The sum or difference of the a and b that the last clock edge sampled. It changes only
  //! at a clock edge, so a and b can change after that edge.
  output logic [63:0] result
);

  // A reset or an enable would put one control signal on all 64 registers.
  always_ff @(posedge clk) begin
    if (SUB) result <= a - b;
    else     result <= a + b;
  end

endmodule
