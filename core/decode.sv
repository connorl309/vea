//! Decode unit of the VEA core.
//!
//! - Decode does not resolve branches. The flags come from a cmp instruction
//!   that can be in execute at the same time. Decode sends the target and the
//!   predicate to execute. Execute sets the redirect for fetch.
//! - Decode reads the registers in the same cycle as it gets the bytes.
//!   The result of the previous instruction is not in the register file yet.
//!   The register file must forward it.
//! - Decode does not stop for an illegal instruction. It sends the class
//!   ILLEGAL to execute. This way an older branch in execute can redirect
//!   fetch first, and then the fault does not occur.

// Macros, not localparams, because execute must use the same codes.
`define DEC_CLASS_NOP     4'd0
`define DEC_CLASS_HALT    4'd1
`define DEC_CLASS_MOV     4'd2
`define DEC_CLASS_ALU     4'd3
`define DEC_CLASS_CMP     4'd4
`define DEC_CLASS_BRANCH  4'd5
`define DEC_CLASS_LOAD    4'd6
`define DEC_CLASS_STORE   4'd7
`define DEC_CLASS_TRAP    4'd8
`define DEC_CLASS_ILLEGAL 4'd9

module vea_decode (
    input wire clk,
    input wire rst,
    input wire flush, //! If 1, the latch drops its instruction. After a redirect, this instruction is on the wrong path.

    input wire insn_valid,
    output logic insn_ready,
    input wire [63:0] insn_pc,
    input wire [95:0] insn_bytes,
    input wire [3:0] insn_len,

    output logic [4:0] rf_raddr_a,
    output logic [4:0] rf_raddr_b,
    output logic [4:0] rf_raddr_c,
    input wire [63:0] rf_rdata_a, //! The value must include a result that is not written back yet.
    input wire [63:0] rf_rdata_b,
    input wire [63:0] rf_rdata_c,

    output logic ex_valid,
    input wire ex_ready,
    output logic [63:0] ex_pc,
    output logic [3:0] ex_class,
    output logic [7:0] ex_alu_op,
    output logic [4:0] ex_rd,
    output logic [63:0] ex_a,
    output logic [63:0] ex_b,
    output logic [63:0] ex_addr, //! Address of a load or a store, or target of a branch. One adder gives both, because no instruction is both.
    output logic [63:0] ex_store_data,
    output logic [2:0] ex_pred,
    output logic [1:0] ex_mem_size,
    output logic ex_mem_sext,
    output logic ex_cmp_signed
  );

  localparam logic [7:0] OP_NOP    = 8'h00;
  localparam logic [7:0] OP_MOV    = 8'h01;
  localparam logic [7:0] OP_ADD    = 8'h10;
  localparam logic [7:0] OP_SUB    = 8'h11;
  localparam logic [7:0] OP_AND    = 8'h12;
  localparam logic [7:0] OP_OR     = 8'h13;
  localparam logic [7:0] OP_NOT    = 8'h14;
  localparam logic [7:0] OP_XOR    = 8'h15;
  localparam logic [7:0] OP_SHL    = 8'h16;
  localparam logic [7:0] OP_SHR    = 8'h17;
  localparam logic [7:0] OP_SAR    = 8'h18;
  localparam logic [7:0] OP_MUL    = 8'h19;
  localparam logic [7:0] OP_DIV    = 8'h1A;
  localparam logic [7:0] OP_CMP    = 8'h20;
  localparam logic [7:0] OP_CMP_S  = 8'h21;
  localparam logic [7:0] OP_BRANCH = 8'h30;
  localparam logic [7:0] OP_JMP    = 8'h31;
  localparam logic [7:0] OP_LD     = 8'h40;
  localparam logic [7:0] OP_ST     = 8'h41;
  localparam logic [7:0] OP_TRAP   = 8'hFE;
  localparam logic [7:0] OP_HALT   = 8'hFF;

  localparam logic [2:0] BR_ALWAYS = 3'd0;
  localparam logic [2:0] BR_LE     = 3'd6;

  localparam logic [3:0] MIN_LEN = 4'd2;
  localparam logic [3:0] MAX_LEN = 4'd12;

  //! Decoded fields of one instruction. The latch holds them for execute.
  typedef struct packed {
            logic [63:0] pc;
            logic [3:0] op_class;
            logic [7:0] alu_op;
            logic [4:0] rd;
            logic [63:0] a;
            logic [63:0] b;
            logic [63:0] addr;
            logic [63:0] store_data;
            logic [2:0] pred;
            logic [1:0] mem_size;
            logic mem_sext;
            logic cmp_signed;
  } decoded_t;

  decoded_t dec;
  decoded_t dec_q;
  logic illegal;
  logic [63:0] addr_base;
  logic [63:0] addr_off;

  wire [7:0] opcode = insn_bytes[95:88];
  wire [3:0] flags  = insn_bytes[83:80];
  wire [7:0] byte2  = insn_bytes[79:72];
  wire [7:0] byte3  = insn_bytes[71:64];
  wire [7:0] byte4  = insn_bytes[63:56];

  wire also_imm = flags[0];
  wire br_imm   = flags[3];
  wire ls_sext  = flags[3];

  //! A register byte comes from the instruction stream, so sanity check its value here
  function automatic logic reg_ok(input logic [7:0] idx);
      reg_ok = (idx < 8'd32);
  endfunction

  //! The next functions get the instruction as arguments. Yosys does not accept a function that reads a module signal.
  //! Some bullshit to extract bytes from Vea's istream
  function automatic logic [7:0] byte_at(input logic [95:0] bytes, input integer idx);
    byte_at = bytes[8 * (11 - idx) +: 8];
  endfunction

  //! The immediate starts at byte "head". It takes the bytes up to the end of the instruction.
  function automatic integer imm_len(input logic [3:0] len, input integer head);
    integer len_int;
    begin
      len_int = {28'h0, len};
      imm_len = (len_int > head) ? (len_int - head) : 0;
    end
  endfunction

  //! An immediate with more than 8 bytes does not fit in a register. Decode reports it as illegal.
  function automatic logic imm_wide(input logic [3:0] len, input integer head);
    imm_wide = imm_len(len, head) > 8;
  endfunction

  //! The function returns 0 for an immediate of 0 bytes or more than 8 bytes.
  //! It does not return early, because an older Yosys does not accept return.
  function automatic logic [63:0] imm_at(input logic [95:0] bytes, input logic [3:0] len, input integer head);
    integer n;
    integer i;
    integer shift;
    logic [63:0] bits;
    logic signed [63:0] left_aligned;
    logic signed [63:0] extended;
    begin
      n = imm_len(len, head);
      shift = 64 - 8 * n;
      bits = 64'h0;
      for (i = 0; i < 8; i = i + 1)
        if (i < n)
          bits = {bits[55:0], byte_at(bytes, head + i)};
      left_aligned = bits << shift;
      //! A ternary with an unsigned operand makes the whole expression unsigned. Then >>> does not extend the sign. So the shift is a separate statement.
      extended = left_aligned >>> shift;
      imm_at = (n == 0 || n > 8) ? 64'h0 : extended;
    end
  endfunction

  //! The immediate starts at byte 2, 3 or 4, depending on the number of register bytes.
  wire [63:0] imm2 = imm_at(insn_bytes, insn_len, 2);
  wire [63:0] imm3 = imm_at(insn_bytes, insn_len, 3);
  wire [63:0] imm4 = imm_at(insn_bytes, insn_len, 4);
  wire imm2_wide = imm_wide(insn_len, 2);
  wire imm3_wide = imm_wide(insn_len, 3);
  wire imm4_wide = imm_wide(insn_len, 4);

  //! This block does not read the register data. The addresses depend only on the instruction bytes.
  //! If one block did both, a tool would see a loop through the register file.
  always_comb begin : ReadPorts
    rf_raddr_a = '0;
    rf_raddr_b = '0;
    rf_raddr_c = '0;

    case (opcode)
      OP_MOV, OP_NOT:
        rf_raddr_a = byte3[4:0];

      OP_CMP, OP_CMP_S:
      begin
        rf_raddr_a = byte2[4:0];
        rf_raddr_b = byte3[4:0];
      end

      OP_ADD, OP_SUB, OP_AND, OP_OR, OP_XOR, OP_SHL, OP_SHR, OP_SAR, OP_MUL, OP_DIV:
      begin
        rf_raddr_a = byte3[4:0];
        rf_raddr_b = byte4[4:0];
      end

      OP_BRANCH, OP_JMP:
        rf_raddr_a = byte2[4:0];

      OP_LD, OP_ST:
      begin
        rf_raddr_a = byte3[4:0];
        rf_raddr_b = byte4[4:0];
        rf_raddr_c = byte2[4:0];
      end

      default:
        ;
    endcase
  end

  always_comb begin : DecodeOp
    dec = '0;
    dec.pc = insn_pc;
    illegal = (insn_len < MIN_LEN) || (insn_len > MAX_LEN);
    addr_base = '0;
    addr_off = '0;

    case (opcode)
      OP_NOP:
        dec.op_class = `DEC_CLASS_NOP;

      OP_HALT:
        dec.op_class = `DEC_CLASS_HALT;

      OP_MOV, OP_NOT:
      begin
        dec.op_class = (opcode == OP_MOV) ? `DEC_CLASS_MOV : `DEC_CLASS_ALU;
        dec.alu_op = (opcode == OP_NOT) ? {4'h0, opcode[3:0]} : 8'h0;
        dec.rd = byte2[4:0];
        dec.a = also_imm ? imm3 : rf_rdata_a;
        illegal = illegal || !reg_ok(byte2) || (also_imm ? imm3_wide : !reg_ok(byte3));
      end

      OP_CMP, OP_CMP_S:
      begin
        dec.op_class = `DEC_CLASS_CMP;
        dec.cmp_signed = (opcode == OP_CMP_S);
        dec.a = rf_rdata_a;
        dec.b = also_imm ? imm3 : rf_rdata_b;
        illegal = illegal || !reg_ok(byte2) || (also_imm ? imm3_wide : !reg_ok(byte3));
      end

      OP_ADD, OP_SUB, OP_AND, OP_OR, OP_XOR, OP_SHL, OP_SHR, OP_SAR, OP_MUL, OP_DIV:
      begin
        dec.op_class = `DEC_CLASS_ALU;
        dec.alu_op = {4'h0, opcode[3:0]};
        dec.rd = byte2[4:0];
        dec.a = rf_rdata_a;
        dec.b = also_imm ? imm4 : rf_rdata_b;
        illegal = illegal || !reg_ok(byte2) || !reg_ok(byte3) || (also_imm ? imm4_wide : !reg_ok(byte4));
      end

      OP_BRANCH, OP_JMP:
      begin
        dec.op_class = `DEC_CLASS_BRANCH;
        dec.pred = (opcode == OP_JMP) ? BR_ALWAYS : flags[2:0];
        if (br_imm)
        begin
          //! A branch offset is relative to the branch itself. A jump target is absolute.
          addr_base = (opcode == OP_JMP) ? 64'h0 : insn_pc;
          addr_off = imm2;
          illegal = illegal || imm2_wide;
        end
        else
        begin
          addr_base = rf_rdata_a;
          illegal = illegal || !reg_ok(byte2);
        end
        illegal = illegal || (dec.pred > BR_LE);
      end

      OP_LD, OP_ST:
      begin
        dec.op_class = (opcode == OP_LD) ? `DEC_CLASS_LOAD : `DEC_CLASS_STORE;
        dec.rd = (opcode == OP_LD) ? byte2[4:0] : 5'h0;
        dec.mem_size = flags[2:1];
        dec.mem_sext = (opcode == OP_LD) && ls_sext;
        addr_base = rf_rdata_a;
        addr_off = also_imm ? imm4 : rf_rdata_b;
        dec.store_data = rf_rdata_c;
        illegal = illegal || !reg_ok(byte2) || !reg_ok(byte3) || (also_imm ? imm4_wide : !reg_ok(byte4));
      end

      OP_TRAP:
      begin
        dec.op_class = `DEC_CLASS_TRAP;
        dec.b = imm2;
        illegal = illegal || imm2_wide;
      end

      default:
        illegal = 1'b1;
    endcase

    dec.addr = addr_base + addr_off;

    if (illegal)
    begin
      dec = '0;
      dec.pc = insn_pc;
      dec.op_class = `DEC_CLASS_ILLEGAL;
    end
  end

  //! Execute can take a new instruction if it is empty or if execute empties it in this cycle. This way the pipeline has no bubble.
  assign insn_ready = !ex_valid || ex_ready;

  always_ff @(posedge clk)
  begin : IdExLatch
    if (rst || flush)
      ex_valid <= 1'b0;
    else if (insn_valid && insn_ready)
    begin
      dec_q <= dec;
      ex_valid <= 1'b1;
    end
    else if (ex_ready)
      ex_valid <= 1'b0;
  end

  assign ex_pc         = dec_q.pc;
  assign ex_class      = dec_q.op_class;
  assign ex_alu_op     = dec_q.alu_op;
  assign ex_rd         = dec_q.rd;
  assign ex_a          = dec_q.a;
  assign ex_b          = dec_q.b;
  assign ex_addr       = dec_q.addr;
  assign ex_store_data = dec_q.store_data;
  assign ex_pred       = dec_q.pred;
  assign ex_mem_size   = dec_q.mem_size;
  assign ex_mem_sext   = dec_q.mem_sext;
  assign ex_cmp_signed = dec_q.cmp_signed;

endmodule
