//! Execute stage for Vea.
//!
//! Execute owns the ALU and the condition codes. It resolves branches and it resolves a
//! load or a store, so vea_writeback needs no special case for them. vea_mem_if turns
//! the generic memory port into SPI SRAM signals. vea_writeback holds the register
//! write port and drives the redirect to Fetch and Decode, so every change to
//! architectural state commits at the same stage.
//!
//! See tb/tb_execute.sv for the testbench.
//!
//! Execute holds the ID/EX register. The register file read and the ALU are then in
//! different cycles. A path through both sets the clock speed of the core.
//!
//! An instruction enters the ID/EX register only when the instruction before it
//! completes. That result is not in the register file yet. It is in Writeback. Decode
//! finds this case and sets ex_fwd_a, ex_fwd_b or ex_fwd_c. Execute then uses fwd_data
//! for that operand. Older results come through the register file bypass.
//!
//! An illegal instruction, an unsupported op, a trap or a misaligned branch target all
//! stop the core for good: no write, no redirect, and no later instruction is ever
//! accepted again, since there is nowhere defined yet for any of them to continue to.
//! TODO: trap needs a real vector once one exists, instead of stopping.

module vea_execute (
  input  logic clk,
  input  logic rst_n,

  //! Decode drives these. ex_illegal means every other ex_ input except ex_pc is
  //! undefined, so Execute must fault on it before it reads them.
  //! Decode must hold ex_valid low while redirect_valid is high.
  input  logic         ex_valid,
  // No fault target exists yet, so no trap or illegal reads the faulting address.
  /* verilator lint_off UNUSEDSIGNAL */
  input  logic [63:0]  ex_pc,
  /* verilator lint_on UNUSEDSIGNAL */
  input  logic [3:0]   ex_alu_op,
  input  logic [63:0]  ex_a,
  input  logic [63:0]  ex_b,
  input  logic [63:0]  ex_c,
  //! High when the operand must come from fwd_data, not from ex_a, ex_b or ex_c.
  input  logic         ex_fwd_a,
  input  logic         ex_fwd_b,
  input  logic         ex_fwd_c,
  input  logic [4:0]   ex_rd,
  input  logic         ex_wr_en,
  input  logic         ex_is_branch,
  input  logic [2:0]   ex_pred,
  input  logic         ex_is_load,
  input  logic         ex_is_store,
  input  logic [1:0]   ex_mem_size,
  input  logic         ex_mem_sext,
  input  logic         ex_is_trap,
  // HALT needs no action here, since Decode's latch already stops Fetch.
  /* verilator lint_off UNUSEDSIGNAL */
  input  logic         ex_is_halt,
  /* verilator lint_on UNUSEDSIGNAL */
  input  logic         ex_illegal,
  //! High when the ID/EX register takes ex_valid and the ex_ inputs at the clock edge.
  output logic         ex_ready,

  //! The last register write from vea_writeback. It must keep its value until the next
  //! write. A forwarded operand reads it for as long as the instruction waits here.
  input  logic [63:0]  fwd_data,
  //! The redirect from vea_writeback. The instruction in the ID/EX register is then on
  //! the wrong path. It entered as the branch completed.
  input  logic         redirect_valid,

  //! The resolved write-back and redirect, one cycle before they reach the register
  //! file and Fetch/Decode. vea_writeback holds both output ports.
  output logic         wb_valid,
  output logic [4:0]   wb_rd,
  output logic [63:0]  wb_data,
  output logic         wb_redirect_valid,
  output logic [63:2]  wb_redirect_pc,

  //! Status for board-level indicators, for example LEDs. Each latches on its first
  //! cycle and holds until reset, so a one-cycle event stays visible.
  output logic         err_illegal,
  output logic         err_unsupported,
  output logic         err_trap,
  output logic         err_unaligned,

  //! Generic load/store port to vea_mem_if. One request in flight at a time, as in
  //! Fetch's imem port. See vea_pkg for the fields of mem_req.
  output logic               mem_req_valid,
  input  logic               mem_req_ready,
  output vea_pkg::mem_req_t  mem_req,
  input  logic               mem_rvalid,
  input  logic [63:0]        mem_rdata
);
  // ---- ID/EX register ----------------------------------------------------------------

  logic        x_valid;
  logic [3:0]  x_alu_op;
  logic [63:0] x_a, x_b, x_c;
  logic        x_fwd_a, x_fwd_b, x_fwd_c;
  logic [4:0]  x_rd;
  logic        x_wr_en, x_is_branch;
  logic [2:0]  x_pred;
  logic        x_is_load, x_is_store;
  logic [1:0]  x_mem_size;
  logic        x_mem_sext, x_is_trap, x_illegal;

  always_ff @(posedge clk) begin
    if (!rst_n || redirect_valid) x_valid <= 1'b0;
    else if (ex_ready)            x_valid <= ex_valid;
  end

  // No reset here. x_valid has a reset, and x_valid gates these fields.
  always_ff @(posedge clk) begin
    if (ex_ready) begin
      x_alu_op    <= ex_alu_op;
      x_a         <= ex_a;
      x_b         <= ex_b;
      x_c         <= ex_c;
      x_fwd_a     <= ex_fwd_a;
      x_fwd_b     <= ex_fwd_b;
      x_fwd_c     <= ex_fwd_c;
      x_rd        <= ex_rd;
      x_wr_en     <= ex_wr_en;
      x_is_branch <= ex_is_branch;
      x_pred      <= ex_pred;
      x_is_load   <= ex_is_load;
      x_is_store  <= ex_is_store;
      x_mem_size  <= ex_mem_size;
      x_mem_sext  <= ex_mem_sext;
      x_is_trap   <= ex_is_trap;
      x_illegal   <= ex_illegal;
    end
  end

  // Decode sets the selects one cycle early. Only this mux is in front of the ALU.
  logic [63:0] op_a, op_b, op_c;

  assign op_a = x_fwd_a ? fwd_data : x_a;
  assign op_b = x_fwd_b ? fwd_data : x_b;
  assign op_c = x_fwd_c ? fwd_data : x_c;

  // ---- ALU -----------------------------------------------------------------------

  logic [63:0] alu_result;
  logic [63:0] alu_add_result;
  logic [3:0]  alu_flags;
  logic        alu_flags_valid;
  logic        alu_unsupported;
  logic        alu_late;

  vea_alu u_alu (
    .clk         (clk),
    .a           (op_a),
    .b           (op_b),
    .op          (x_alu_op),
    .result      (alu_result),
    .add_result  (alu_add_result),
    .flags       (alu_flags),
    .flags_valid (alu_flags_valid),
    .unsupported (alu_unsupported),
    .late        (alu_late)
  );

  //! Any fault stops the core for good, so this also gates out whatever instruction is
  //! stuck behind it: without this, a legal instruction that entered the ID/EX register
  //! the same cycle the fault latched would complete, since ex_ready alone (below) only
  //! blocks the *next* one from being accepted.
  logic stopped;

  assign stopped = err_illegal | err_unsupported | err_trap | err_unaligned;

  //! High for an instruction that may complete. Everything below reads this, not
  //! x_valid. An instruction on the wrong path must not change state.
  logic live;

  assign live = x_valid && !redirect_valid && !stopped;

  //! High for a legal, live instruction. ex_illegal makes the other fields undefined.
  logic valid_op;

  assign valid_op = live && !x_illegal;

  // ---- Condition codes -------------------------------------------------------------

  //! CMP and CMP_S write this register so a later branch can read it. Its layout matches
  //! the simulator's condition codes; only zero, neg and overflow feed a predicate below.
  /* verilator lint_off UNUSEDSIGNAL */
  logic [3:0] cc;
  /* verilator lint_on UNUSEDSIGNAL */

  always_ff @(posedge clk) begin
    if (!rst_n)
      cc <= '0;
    else if (valid_op && alu_flags_valid)
      cc <= alu_flags;
  end

  // ---- Branch resolution ------------------------------------------------------------

  localparam logic [2:0] PRED_ALWAYS = 3'd0;
  localparam logic [2:0] PRED_EQ     = 3'd1;
  localparam logic [2:0] PRED_NE     = 3'd2;
  localparam logic [2:0] PRED_LT     = 3'd3;
  localparam logic [2:0] PRED_GE     = 3'd4;
  localparam logic [2:0] PRED_GT     = 3'd5;
  localparam logic [2:0] PRED_LE     = 3'd6;

  logic cc_zero, cc_lt, branch_taken;

  assign cc_zero = cc[3];
  assign cc_lt   = cc[2] ^ cc[0];

  always_comb begin
    unique case (x_pred)
      PRED_ALWAYS: branch_taken = 1'b1;
      PRED_EQ:     branch_taken = cc_zero;
      PRED_NE:     branch_taken = ~cc_zero;
      PRED_LT:     branch_taken = cc_lt;
      PRED_GE:     branch_taken = ~cc_lt;
      PRED_GT:     branch_taken = ~cc_zero & ~cc_lt;
      PRED_LE:     branch_taken = cc_zero | cc_lt;
      // Predicate 7 is undefined; Decode faults on it for a B, and JMP always
      // uses PRED_ALWAYS.
      default:     branch_taken = 1'b0;
    endcase
  end

  // ---- Multiply ----------------------------------------------------------------------

  // This value must match vea_alu's ALU_MUL.
  localparam logic [3:0] ALU_MUL = 4'h9;

  logic        is_mul, mul_start, mul_valid;
  logic [63:0] mul_result;

  assign is_mul = x_alu_op == ALU_MUL;

  vea_mul u_mul (
    .clk    (clk),
    .rst_n  (rst_n),
    .start  (mul_start),
    .a      (op_a),
    .b      (op_b),
    .valid  (mul_valid),
    .result (mul_result)
  );

  // ---- Memory and multiply wait --------------------------------------------------------

  //! ADD and SUB take one wait cycle. Their adder has a result register. A load, a store
  //! and a branch use ADD for the address or the target, so they wait too. A load or a
  //! store then takes two more cycles: request, then reply. MUL takes several cycles, in
  //! vea_mul. All of these hold Execute the same way. Decode stages the next instruction
  //! while one of them runs.
  typedef enum logic [1:0] { S_IDLE, S_ADD_WAIT, S_MEM_WAIT, S_MUL_WAIT } wait_state_t;

  wait_state_t state, next_state;
  logic        late_op, mem_op, mem_req_taken, mul_op;

  assign late_op       = valid_op && alu_late;
  assign mem_op        = valid_op && (x_is_load || x_is_store);
  assign mem_req_taken = mem_req_valid && mem_req_ready;
  assign mul_op        = valid_op && is_mul;

  always_comb begin
    next_state = state;
    unique case (state)
      S_IDLE: begin
        if (late_op)     next_state = S_ADD_WAIT;
        else if (mul_op) next_state = S_MUL_WAIT;
      end
      // Decode gives every load and store the ADD operation. So a memory operation is
      // always here, with its address in alu_add_result.
      S_ADD_WAIT: begin
        if (mem_req_taken) next_state = S_MEM_WAIT;
        else if (!mem_op)  next_state = S_IDLE;
      end
      S_MEM_WAIT: if (mem_rvalid) next_state = S_IDLE;
      S_MUL_WAIT: if (mul_valid)  next_state = S_IDLE;
    endcase
  end

  always_ff @(posedge clk) begin
    if (!rst_n) state <= S_IDLE;
    else        state <= next_state;
  end

  assign mem_req_valid = (state == S_ADD_WAIT) && mem_op;
  assign mul_start      = (state == S_IDLE) && mul_op;
  // A plain concatenation, field order MSB first as in vea_pkg::mem_req_t
  assign mem_req        = {alu_add_result, x_is_store, x_mem_size, op_c};

  // Matches the opinfo size field (LS_SIZE_D/B/H/W). A narrow load sits low in
  // mem_rdata; the rest is sign- or zero-extended.
  localparam logic [1:0] SIZE_D = 2'b00;
  localparam logic [1:0] SIZE_B = 2'b01;
  localparam logic [1:0] SIZE_H = 2'b10;
  localparam logic [1:0] SIZE_W = 2'b11;

  logic [63:0] load_value;

  always_comb begin
    unique case (x_mem_size)
      SIZE_B:  load_value = x_mem_sext ? {{56{mem_rdata[7]}},  mem_rdata[7:0]}
                                        : {56'b0, mem_rdata[7:0]};
      SIZE_H:  load_value = x_mem_sext ? {{48{mem_rdata[15]}}, mem_rdata[15:0]}
                                        : {48'b0, mem_rdata[15:0]};
      SIZE_W:  load_value = x_mem_sext ? {{32{mem_rdata[31]}}, mem_rdata[31:0]}
                                        : {32'b0, mem_rdata[31:0]};
      SIZE_D:  load_value = mem_rdata;
    endcase
  end

  // ---- Handshake, redirect and write-back handoff ---------------------------------------

  //! High the one cycle an instruction retires: right away, or on the reply for a
  //! load/store.
  logic completing;

  always_comb begin
    unique case (state)
      S_IDLE:     completing = !late_op && !mul_op;
      S_ADD_WAIT: completing = !mem_op;
      S_MEM_WAIT: completing = mem_rvalid;
      S_MUL_WAIT: completing = mul_valid;
    endcase
  end

  //! Execute stops asserting ex_ready once stopped, so Decode's stage register can
  //! never empty, so its frame_ready never asserts again, so Fetch never requests
  //! another frame. No separate stop wire to Fetch or Decode is needed. The err_ bits
  //! are registered, so the faulting instruction itself still completes this cycle;
  //! only the next one is refused.
  assign ex_ready = (!x_valid || completing) && !stopped;

  //! B and JMP form a target from a register or an immediate, either of which can land
  //! off a 4-byte boundary. Fetch only ever sees the aligned bits (redirect_pc is
  //! [63:2]), so a misaligned target must fault here, before the low bits are dropped.
  logic pc_misaligned;

  // The target always comes from ADD. Reading the ADD result directly keeps the shifters
  // out of the path to the redirect, the address check and the memory address.
  assign pc_misaligned     = valid_op && x_is_branch && branch_taken && |alu_add_result[1:0];
  assign wb_redirect_valid = completing && valid_op && x_is_branch && branch_taken
                            && !pc_misaligned;
  assign wb_redirect_pc    = alu_add_result[63:2];

  // DIV decodes as legal, but vea_alu has no silicon for it. It raises alu_unsupported
  // instead. That must not reach the register file.
  // MUL decodes as legal too. Its result comes from vea_mul, not vea_alu.
  assign wb_valid = completing && valid_op && x_wr_en && !alu_unsupported;
  assign wb_rd    = x_rd;
  assign wb_data  = x_is_load ? load_value
                   : is_mul   ? mul_result
                   :            alu_result;

  // ---- Status indicators ---------------------------------------------

  always_ff @(posedge clk) begin
    if (!rst_n) begin
      err_illegal     <= 1'b0;
      err_unsupported <= 1'b0;
      err_trap        <= 1'b0;
      err_unaligned   <= 1'b0;
    end else begin
      if (completing && live && x_illegal)           err_illegal     <= 1'b1;
      if (completing && valid_op && alu_unsupported) err_unsupported <= 1'b1;
      if (completing && valid_op && x_is_trap)       err_trap        <= 1'b1;
      if (completing && pc_misaligned)                err_unaligned   <= 1'b1;
    end
  end

endmodule
