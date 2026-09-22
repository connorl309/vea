//! Decode stage for Vea. It has two stages with a register between them.
//!
//! Stage 1 decodes the frame: the operation, the register numbers, the immediate and the
//! faults. Stage 2 reads the register file and selects the operands. The register between
//! the stages keeps the frame decode and the register read in different cycles, so no
//! path runs from the frame bytes through the register file.
//!
//! The register file has two read ports, but a store with an index register needs three
//! values. Stage 2 keeps such a store for one extra cycle to read the value of the store.
//!
//! Decode has no adder. Execute finds every load address, store address and branch
//! target with the ALU adder, so the core needs only one 64-bit adder. Decode only
//! selects the operands of that adder.
//!
//! The ID/EX register belongs to Execute, as the result register of the ALU does.
//!
//! The stage register holds register numbers and no register values. A value in that
//! register would be old when the instruction waits behind an instruction that stalls.
//!
//! The register file must return the newest value of a register, also when the result
//! is not written back yet. Decode does not stall on a register hazard.
//!
//! When ex_illegal is high, the other ex_ outputs are undefined, except ex_pc. This lets
//! the opcode decode ignore the opcodes that are not defined.

// SystemVerilog notes - <operator>|<variable> is a reduction operator!
// SV will automatically convert this statement into the appropriate nesting
// in logic, so if I do something like &|frame_bytes[10:15], the result is
// equivalent to a 6-input AND gate.

module vea_decode #(
  parameter int MAX_INSN_BYTES = 13
) (
  input  logic clk,
  input  logic rst_n,

  input  logic                        frame_valid,
  output logic                        frame_ready,
  input  logic [63:0]                 frame_pc,
  input  logic [8*MAX_INSN_BYTES-1:0] frame_bytes,
  input  logic                        frame_bad_len,

  //! A redirect drops the frame in the same cycle, and it empties the stage register,
  //! because that instruction is on the wrong path too. It also clears the halt latch,
  //! because a halt on the wrong path of a taken branch is not a halt.
  input  logic redirect_valid,
  //! Decode holds this high after it accepts a halt. Fetch must not read the bytes
  //! after a halt.
  output logic halt,

  //! The read ports have no register hazard logic. The register file owner must forward
  //! the results that are not yet written back.
  output logic [4:0]  rf_raddr_a,
  output logic [4:0]  rf_raddr_b,
  input  logic [63:0] rf_rdata_a,
  input  logic [63:0] rf_rdata_b,

  output logic        ex_valid,
  input  logic        ex_ready,
  //! The address of this instruction. It is valid also when ex_illegal is high, because
  //! a fault reports it.
  output logic [63:0] ex_pc,
  //! Execute uses the ADD result as the address of a load or store, and as the target
  //! of a branch or jump. The ALU sets the condition codes for CMP and CMP_S only.
  output logic [3:0]  ex_alu_op,
  output logic [63:0] ex_a,
  output logic [63:0] ex_b,
  //! The value that a store writes. The ALU does not use it.
  output logic [63:0] ex_c,
  output logic [4:0]  ex_rd,
  output logic        ex_wr_en,
  //! High for b and for jmp. A jmp has the predicate 0 (always), so Execute needs no
  //! separate case for it.
  output logic        ex_is_branch,
  output logic [2:0]  ex_pred,
  output logic        ex_is_load,
  output logic        ex_is_store,
  output logic [1:0]  ex_mem_size,
  output logic        ex_mem_sext,
  //! The trap vector is in ex_a.
  output logic        ex_is_trap,
  output logic        ex_is_halt,
  output logic        ex_illegal
);

  // These values must match vea_alu. The low nibble of the CMP opcodes is 0 and 1,
  // which are already ADD and SUB.
  localparam logic [3:0] ALU_ADD   = 4'h0;
  localparam logic [3:0] ALU_CMP   = 4'hB;
  localparam logic [3:0] ALU_CMP_S = 4'hC;

  // The shifter of the immediate has a zero pad below the last frame byte. With the pad,
  // the shift amount is the complement of the length, and no subtractor is necessary.
  localparam int PAD_BYTES = 15 - MAX_INSN_BYTES;
  localparam int TAIL_BITS = 8 * (MAX_INSN_BYTES - 4);

  // Stage 1 makes all of these fields from the frame alone. Only the values of the
  // registers are missing, because stage 2 reads them.
  typedef struct packed {
    logic [63:0] pc;
    logic [63:0] imm;
    // The selection of the operands needs the register values. Stage 1 has no values, so
    // it passes the selectors on to stage 2.
    logic        imm_form;
    logic        src_in_a;
    logic        is_b;
    // The store with an index register needs an extra cycle in stage 2.
    logic        idx_store;
    logic [3:0]  alu_op;
    // A store has no destination. Its value register is at the same byte, so stage 2
    // uses this field as the address of the value.
    logic [4:0]  rd;
    logic [4:0]  raddr_a;
    logic [4:0]  raddr_b;
    logic        wr_en;
    logic        is_bj;
    logic [2:0]  pred;
    logic        is_load;
    logic        is_store;
    logic [1:0]  mem_size;
    logic        mem_sext;
    logic        is_trap;
    logic        is_halt;
    logic        illegal;
  } insn_t;

  // The result of stage 1, and the register that holds it for stage 2. Stage 1 must not
  // read s1, so the tools see no loop through the struct.
  insn_t s1;
  insn_t s2;

  logic [7:0]           opcode, opinfo, byte2, byte3, byte4;
  logic [TAIL_BITS-1:0] tail;
  logic [3:0]           len;

  assign {opcode, opinfo, byte2, byte3, tail} = frame_bytes;
  assign byte4 = tail[TAIL_BITS-1 -: 8];
  assign len   = opinfo[7:4];

  // ---- Opcode ----------------------------------------------------------------------

  // Each group test reads only the opcode bits that separate its group from the other
  // defined groups, so the tests stay small. A test can be true for an opcode that is
  // not defined. The signal legal_opcode then raises the fault.
  logic in_misc, in_alu, in_cmp, in_ctl, in_mem;
  logic is_mov, is_not, is_b, is_jmp, is_bj, is_load, is_store, is_trap, is_halt;

  // NOP and MOV. Only these two defined opcodes have bits 6:4 all zero.
  assign in_misc = opcode[6:4] == 3'b000;
  // ADD to DIV. Only this group has bit 4 set and bit 5 clear.
  assign in_alu  = opcode[5:4] == 2'b01;
  // CMP and CMP_S.
  assign in_cmp  = opcode[5:4] == 2'b10;
  // B, JMP, TRAP and HALT. Bit 7 separates the first two from the last two.
  assign in_ctl  = opcode[5:4] == 2'b11;
  // LD and ST. Bit 6 separates this group from TRAP and HALT, which also have bit 4 set.
  assign in_mem  = opcode[6:4] == 3'b100;

  // NOP is the other opcode of its group. It has no output, so no signal decodes it.
  assign is_mov   = in_misc & opcode[0];
  // NOT is in the ALU group, but it has one source operand, as MOV has. The format
  // logic below must find it.
  assign is_not   = in_alu & (opcode[3:0] == 4'h4);
  // B and JMP use the same operand format. Execute must not need to tell them apart.
  assign is_bj    = in_ctl & ~opcode[7];
  // B has a target relative to its own address. JMP has an absolute target. Bit 0
  // separates them.
  assign is_b     = is_bj & ~opcode[0];
  assign is_jmp   = is_bj & opcode[0];
  // Bit 0 separates a load from a store.
  assign is_load  = in_mem & ~opcode[0];
  assign is_store = in_mem & opcode[0];
  // Only TRAP and HALT have bit 7 set, so bit 7 and bit 0 are enough to find them.
  assign is_trap  = opcode[7] & ~opcode[0];
  assign is_halt  = opcode[7] & opcode[0];

  logic legal_opcode;

  // This is the only test of the whole opcode. All other decode signals depend on it.
  always_comb begin
    case (opcode[7:4])
      // NOP and MOV, CMP and CMP_S, B and JMP, LD and ST. Each group has two opcodes.
      4'h0, 4'h2, 4'h3, 4'h4: legal_opcode = ~|opcode[3:1];
      // ADD to DIV are the opcodes 0 to A.
      4'h1:                   legal_opcode = opcode[3:0] <= 4'hA;
      // TRAP and HALT.
      4'hF:                   legal_opcode = &opcode[3:1];
      default:                legal_opcode = 1'b0;
    endcase
  end

  // ---- Operand layout ----------------------------------------------------------------

  // The last operand starts at byte 2, 3 or 4. It is a register or an immediate.
  logic opnd_at_2, opnd_at_3, opnd_at_4;
  logic imm_form;

  // The immediate shifter and the register checks need the byte where the last operand
  // starts. HALT is in this group, but it has no operand, and no output uses the value.
  // One operand: the target of B and JMP, or the vector of TRAP.
  assign opnd_at_2 = in_ctl;
  // Two operands: a destination and a source (MOV and NOT), or CMP with its two sources.
  assign opnd_at_3 = is_mov | is_not | in_cmp;
  // Three operands: the ALU group, and LD and ST. NOP has no operand, so it must not be
  // in this group. Its register bytes hold the start of the next instruction.
  assign opnd_at_4 = (in_alu & ~is_not) | in_mem;

  // The flag for an immediate is bit 3 for B and JMP, because bit 0 of their flags is
  // already a predicate bit. All other opcodes use bit 0. A TRAP always has an
  // immediate, and its flags are not defined.
  assign imm_form = in_ctl ? (opinfo[3] | is_trap) : opinfo[0];

  // ---- Immediate ---------------------------------------------------------------------

  // The immediate ends at byte len-1 for every format. Decode fills the frame bytes
  // before the immediate with the sign, and then shifts the frame down by whole bytes.
  // The shift moves the sign extension in with the immediate. It needs no mask for each
  // byte, and no subtractor for the immediate length.
  logic         sign;
  logic [7:0]   fill, lead2, lead3;
  logic [167:0] wide;
  logic [63:0]  imm;

  // An immediate of length zero has the value zero.
  always_comb begin
    if (opnd_at_2)      sign = (len > 4'd2) & byte2[7];
    else if (opnd_at_3) sign = (len > 4'd3) & byte3[7];
    else                sign = (len > 4'd4) & byte4[7];
  end

  assign fill  = {8{sign}};
  // Byte 2 and byte 3 are part of the immediate only for the formats that start the last
  // operand there. In the other formats they hold a register number, and the shift
  // must move the sign in instead.
  assign lead2 = opnd_at_2 ? byte2 : fill;
  assign lead3 = (opnd_at_2 | opnd_at_3) ? byte3 : fill;
  // The eight fill bytes cover the opcode, the opinfo, and six bytes above the frame.
  // The opcode and the opinfo are never part of an immediate. The six bytes are the
  // bytes of an 8-byte immediate that a 2-byte immediate does not have.
  assign wide  = {{8{fill}}, lead2, lead3, tail, {(8*PAD_BYTES){1'b0}}};
  // The bytes after the instruction move out at the bottom, so junk after the
  // instruction cannot reach the result.
  assign imm   = 64'(wide >> {~len, 3'b000});

  // ---- Register numbers --------------------------------------------------------------

  logic [4:0] raddr_a, raddr_b;

  // Bit 5 of the opcode is set for cmp, b, jmp, trap and halt only. Their register is
  // one byte earlier than in the other formats. A mux on the 5-bit address costs less
  // than a mux on the 64-bit data.
  assign raddr_a = opcode[5] ? byte2[4:0] : byte3[4:0];

  // With an immediate, the second source is the immediate, so port B has no other use.
  // A store with a displacement reads its value there. The value register is at byte 2.
  always_comb begin
    if (is_store & imm_form) raddr_b = byte2[4:0];
    else if (opcode[5])      raddr_b = byte3[4:0];
    else                     raddr_b = byte4[4:0];
  end

  // ---- ALU operation -----------------------------------------------------------------

  logic [3:0] alu_op;

  // The low nibble of an ALU opcode is the ALU operation, so no translation is needed.
  // All other instructions that use the ALU need an ADD: MOV and the target or address
  // calculations. CMP has its own values, because its low nibble is ADD or SUB.
  always_comb begin
    if (in_alu)      alu_op = opcode[3:0];
    else if (in_cmp) alu_op = opcode[0] ? ALU_CMP_S : ALU_CMP;
    else             alu_op = ALU_ADD;
  end

  // ---- Faults ------------------------------------------------------------------------

  logic uses_r2, uses_r3, uses_r4;
  logic reg_bad, imm_bad, len_bad, pred_bad;
  logic illegal;

  // A byte that the format does not use as a register can hold anything, for example
  // the start of an immediate. Only the used register bytes may raise a fault.
  // Byte 2 is a register in all formats with a destination, and for the register
  // target of B and JMP.
  assign uses_r2 = opnd_at_3 | opnd_at_4 | (is_bj & ~imm_form);
  // Byte 3 is the first source, except in a format with the last operand at byte 3.
  assign uses_r3 = opnd_at_4 | (opnd_at_3 & ~imm_form);
  // Byte 4 is the second source or the index, and only when it is not an immediate.
  assign uses_r4 = opnd_at_4 & ~imm_form;

  // The register file has 32 registers. The upper three bits of a register byte must
  // be zero, because Decode sends only the lower five bits to the register file.
  assign reg_bad  = (uses_r2 & (|byte2[7:5]))
                  | (uses_r3 & (|byte3[7:5]))
                  | (uses_r4 & (|byte4[7:5]));
  // Fetch flags a length below 2. The longest instruction has 12 bytes: an opcode, an
  // opinfo, two register bytes and an immediate of 8 bytes.
  assign len_bad  = frame_bad_len | (len > 4'd12);
  // An immediate has at most 8 bytes. The format with the last operand at byte 4 cannot
  // break this when len_bad is low. The other formats start earlier, so they can.
  // HALT is not in this test, because it has no immediate.
  assign imm_bad  = imm_form & (((is_bj | is_trap) & (len > 4'd10))
                              | (opnd_at_3 & (len > 4'd11)));
  // The predicate 7 is not defined. A JMP does not use its predicate bits.
  assign pred_bad = is_b & (&opinfo[2:0]);

  // Decode reports the fault to Execute and does not stall, so a bad frame cannot
  // block the pipeline.
  assign illegal = ~legal_opcode | len_bad | imm_bad | pred_bad | reg_bad;

  // ---- Stage 1 result ----------------------------------------------------------------

  // Fetch has already moved its PC to the next instruction. The frame holds the only
  // copy of the address that a fault must report.
  assign s1.pc       = frame_pc;
  assign s1.imm      = imm;
  assign s1.imm_form = imm_form;
  // MOV, NOT, JMP and TRAP pass one value through the ALU. This value goes in A, as NOT
  // reads only A. B must be zero for MOV, because MOV is an ADD.
  assign s1.src_in_a = is_mov | is_not | is_jmp | is_trap;
  assign s1.is_b     = is_b;
  // An immediate replaces the index register. Only the form with an index register has
  // three register sources.
  assign s1.idx_store = is_store & ~imm_form;
  assign s1.alu_op    = alu_op;

  // The destination is at byte 2 in every format that has one. Execute uses ex_wr_en to
  // find out if the instruction has a destination. The value of a store is at byte 2 too.
  assign s1.rd        = byte2[4:0];
  assign s1.raddr_a   = raddr_a;
  assign s1.raddr_b   = raddr_b;
  // CMP, ST, B, JMP, TRAP, HALT and NOP write no register. The write of a load comes
  // later from the memory, but Execute must know now that it has a destination.
  assign s1.wr_en     = is_mov | in_alu | is_load;
  // Execute needs one signal for B and JMP, because the ALU makes the target for both.
  assign s1.is_bj     = is_bj;
  // The predicate encoding is the same as the flags of the opinfo, so B needs no
  // translation. A JMP has zero in these bits in valid code, but the simulator does
  // not test them. The predicate is forced to always so that the two agree.
  assign s1.pred      = is_jmp ? 3'b000 : opinfo[2:0];
  assign s1.is_load   = is_load;
  assign s1.is_store  = is_store;
  // The size and the sign extension are bits of the opinfo. Execute reads them directly,
  // so Decode needs no logic for them. They have a meaning only for a load or a store.
  assign s1.mem_size  = opinfo[2:1];
  assign s1.mem_sext  = opinfo[3];
  // The vector of a TRAP is in ex_a, because the vector is a source that goes in A.
  assign s1.is_trap   = is_trap;
  assign s1.is_halt   = is_halt;
  assign s1.illegal   = illegal;

  // ---- Stage register ----------------------------------------------------------------

  logic        s2_valid, value_done;
  logic [63:0] store_value;
  logic        need_value, read_value, can_issue, s2_load;

  // The register file has two read ports, so a store with an index register reads its
  // value in a cycle of its own.
  assign need_value = s2_valid & s2.idx_store & ~value_done;
  // Stage 2 reads the value only when Execute is ready. When Execute is not ready, an
  // older instruction can still work, and the register file does not have its result.
  // The store would then keep an old value.
  assign read_value = need_value & ex_ready;
  assign can_issue  = s2_valid & ~need_value;

  // A frame can enter when the register is empty. It can also enter when Execute takes
  // the instruction in the register in the same cycle, or the pipeline would send an
  // instruction only in every second cycle.
  assign frame_ready = ~s2_valid | (can_issue & ex_ready);
  // After a redirect, the frame is on the wrong path of a taken branch. The register must
  // not take it. Fetch drops the frame by itself in the same cycle.
  assign s2_load     = frame_valid & frame_ready & ~redirect_valid;

  always_ff @(posedge clk) begin
    if (!rst_n || redirect_valid)
      s2_valid <= 1'b0;
    else if (s2_load)
      s2_valid <= 1'b1;
    else if (can_issue && ex_ready)
      s2_valid <= 1'b0;
  end

  // These registers have no reset. Stage 2 uses them only when s2_valid is high, and a
  // load of the stage register clears value_done.
  always_ff @(posedge clk) begin
    if (s2_load) begin
      s2         <= s1;
      value_done <= 1'b0;
    end else if (read_value) begin
      value_done <= 1'b1;
    end

    if (read_value) store_value <= rf_rdata_a;
  end

  // ---- Stage 2: register read --------------------------------------------------------

  assign rf_raddr_a = need_value ? s2.rd : s2.raddr_a;
  assign rf_raddr_b = s2.raddr_b;

  // ---- Stage 2: operands -------------------------------------------------------------

  always_comb begin
    // ALU, CMP, LD and ST: A is the first source or the base register. B is the
    // second source, the index register, or the displacement.
    ex_a = rf_rdata_a;
    ex_b = s2.imm_form ? s2.imm : rf_rdata_b;

    if (s2.src_in_a) begin
      // A JMP with an absolute immediate target is the same as a MOV of the target.
      // The ADD result is then the target.
      ex_a = s2.imm_form ? s2.imm : rf_rdata_a;
      ex_b = '0;
    end else if (s2.is_b) begin
      // An immediate target is relative to the address of the B itself. A register
      // target is absolute, so it needs no address.
      ex_a = s2.imm_form ? s2.pc : rf_rdata_a;
      ex_b = s2.imm_form ? s2.imm : 64'b0;
    end
  end

  // The adder of the address needs both A and B, so the value of a store has its own
  // path. A store with a displacement has the value on port B, because B has no other
  // use in that form. The other instructions do not use this output.
  assign ex_c = s2.idx_store ? store_value : rf_rdata_b;

  // ---- Stage 2: outputs --------------------------------------------------------------

  // After a redirect, the instruction in the register is on the wrong path of a taken
  // branch. Execute must not see it. The clock edge empties the register.
  assign ex_valid = can_issue & ~redirect_valid;

  // No register value decides these fields, so stage 1 has made them.
  assign ex_pc        = s2.pc;
  assign ex_alu_op    = s2.alu_op;
  assign ex_rd        = s2.rd;
  assign ex_wr_en     = s2.wr_en;
  assign ex_is_branch = s2.is_bj;
  assign ex_pred      = s2.pred;
  assign ex_is_load   = s2.is_load;
  assign ex_is_store  = s2.is_store;
  assign ex_mem_size  = s2.mem_size;
  assign ex_mem_sext  = s2.mem_sext;
  assign ex_is_trap   = s2.is_trap;
  assign ex_is_halt   = s2.is_halt;
  assign ex_illegal   = s2.illegal;

  // ---- Halt latch --------------------------------------------------------------------

  // The latch changes when the register takes the halt, not when Execute takes it. The
  // frame leaves Fetch when the register takes it, so Fetch would read the bytes after
  // the halt if the latch waited for Execute. An illegal halt does not stop Fetch,
  // because Execute must fault on it.
  always_ff @(posedge clk) begin
    if (!rst_n || redirect_valid)
      halt <= 1'b0;
    else if (s2_load && is_halt && !illegal)
      halt <= 1'b1;
  end

endmodule
