`define ALU_ADD 8'h0
`define ALU_SUB 8'h1
`define ALU_AND 8'h2
`define ALU_OR  8'h3
`define ALU_NOT 8'h4
`define ALU_XOR 8'h5
`define ALU_SHL 8'h6
`define ALU_SHR 8'h7
`define ALU_SAR 8'h8
`define ALU_MUL 8'h9
`define ALU_DIV 8'hA

module vea_alu (
    input wire clk, //! Input clock signal
    input [63:0] in_a, //! First input to ALU
    input [63:0] in_b, //! Second input to ALU
    input [7:0] operation, //! Low nibble of opcode byte from decode
    output logic [63:0] result //! Answer
  );

  always_ff @(posedge clk)
  begin : AluOpPath
    case (operation)
      `ALU_ADD:
        result <= in_a + in_b;
      `ALU_SUB:
        result <= in_a - in_b;
      `ALU_AND:
        result <= in_a & in_b;
      `ALU_OR:
        result <= in_a | in_b;
      `ALU_NOT:
        result <= ~in_a;
      `ALU_XOR:
        result <= in_a ^ in_b;
      `ALU_SHL:
        result <= in_a << in_b[5:0];
      `ALU_SHR:
        result <= in_a >> in_b[5:0];
      `ALU_SAR:
        result <= $signed(in_a) >>> in_b[5:0];
      // TODO: replace with a real (and not shitty) multiply/divide unit. Until then,
      // kill the simulation rather than return a result no hardware will produce.
      `ALU_MUL:
      begin
        $fatal(1, "vea_alu: MUL not implemented (a=%h b=%h)", in_a, in_b);
        result <= in_a * in_b;
      end
      `ALU_DIV:
      begin
        $fatal(1, "vea_alu: DIV not implemented (a=%h b=%h)", in_a, in_b);
        result <= in_a / in_b;
      end
      default:
        ;
    endcase
  end

endmodule
