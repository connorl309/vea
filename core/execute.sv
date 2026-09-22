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
//! An illegal instruction, an unsupported op, a trap or a misaligned branch target all
//! stop the core for good: no write, no redirect, and no later instruction is ever
//! accepted again, since there is nowhere defined yet for any of them to continue to.
//! TODO: trap needs a real vector once one exists, instead of stopping.

module vea_execute (
  input  logic clk,
  input  logic rst_n,

  //! Decode drives these. ex_illegal means every other ex_ input except ex_pc is
  //! undefined, so Execute must fault on it before it reads them.
  input  logic         ex_valid,
  // No fault target exists yet, so no trap or illegal reads the faulting address.
  /* verilator lint_off UNUSEDSIGNAL */
  input  logic [63:0]  ex_pc,
  /* verilator lint_on UNUSEDSIGNAL */
  input  logic [3:0]   ex_alu_op,
  input  logic [63:0]  ex_a,
  input  logic [63:0]  ex_b,
  input  logic [63:0]  ex_c,
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
  output logic         ex_ready,

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
  // ---- ALU -----------------------------------------------------------------------

  logic [63:0] alu_result;
  logic [3:0]  alu_flags;
  logic        alu_flags_valid;
  logic        alu_unsupported;

  vea_alu u_alu (
    .a           (ex_a),
    .b           (ex_b),
    .op          (ex_alu_op),
    .result      (alu_result),
    .flags       (alu_flags),
    .flags_valid (alu_flags_valid),
    .unsupported (alu_unsupported)
  );

  //! Any fault stops the core for good, so this also gates out whatever instruction is
  //! stuck behind it: without this, a legal instruction that Decode already handed to
  //! Execute the same cycle the fault latched would keep re-completing forever, since
  //! ex_ready alone (below) only blocks the *next* one from being accepted.
  logic stopped;

  assign stopped = err_illegal | err_unsupported | err_trap | err_unaligned;

  //! High for a legal, present instruction, and only once the core has not stopped.
  //! Everything below reads this instead of ex_valid, since ex_illegal makes the other
  //! ex_ fields undefined.
  logic valid_op;

  assign valid_op = ex_valid && !ex_illegal && !stopped;

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
    unique case (ex_pred)
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

  // ---- Memory ------------------------------------------------------------------------

  //! A load or a store takes two+ cycles: request, then reply.
  typedef enum logic { S_IDLE, S_MEM_WAIT } mem_state_t;

  mem_state_t state, next_state;
  logic       mem_op, mem_req_taken;

  assign mem_op        = valid_op && (ex_is_load || ex_is_store);
  assign mem_req_taken = mem_req_valid && mem_req_ready;

  always_comb begin
    next_state = state;
    unique case (state)
      S_IDLE:     if (mem_req_taken) next_state = S_MEM_WAIT;
      S_MEM_WAIT: if (mem_rvalid)    next_state = S_IDLE;
    endcase
  end

  always_ff @(posedge clk) begin
    if (!rst_n) state <= S_IDLE;
    else        state <= next_state;
  end

  assign mem_req_valid = (state == S_IDLE) && mem_op;
  // A plain concatenation, field order MSB first as in vea_pkg::mem_req_t
  assign mem_req       = {alu_result, ex_is_store, ex_mem_size, ex_c};

  // Matches the opinfo size field (LS_SIZE_D/B/H/W). A narrow load sits low in
  // mem_rdata; the rest is sign- or zero-extended.
  localparam logic [1:0] SIZE_D = 2'b00;
  localparam logic [1:0] SIZE_B = 2'b01;
  localparam logic [1:0] SIZE_H = 2'b10;
  localparam logic [1:0] SIZE_W = 2'b11;

  logic [63:0] load_value;

  always_comb begin
    unique case (ex_mem_size)
      SIZE_B:  load_value = ex_mem_sext ? {{56{mem_rdata[7]}},  mem_rdata[7:0]}
                                         : {56'b0, mem_rdata[7:0]};
      SIZE_H:  load_value = ex_mem_sext ? {{48{mem_rdata[15]}}, mem_rdata[15:0]}
                                         : {48'b0, mem_rdata[15:0]};
      SIZE_W:  load_value = ex_mem_sext ? {{32{mem_rdata[31]}}, mem_rdata[31:0]}
                                         : {32'b0, mem_rdata[31:0]};
      SIZE_D:  load_value = mem_rdata;
    endcase
  end

  // ---- Handshake, redirect and write-back handoff ---------------------------------------

  //! High the one cycle an instruction retires: right away, or on the reply for a
  //! load/store.
  logic completing;

  assign completing = (state == S_IDLE) ? !mem_op : mem_rvalid;

  //! Execute stops asserting ex_ready once stopped, so Decode's stage register can
  //! never empty, so its frame_ready never asserts again, so Fetch never requests
  //! another frame. No separate stop wire to Fetch or Decode is needed. The err_ bits
  //! are registered, so the faulting instruction itself still completes this cycle;
  //! only the next one is refused.
  assign ex_ready = completing && !stopped;

  //! B and JMP form a target from a register or an immediate, either of which can land
  //! off a 4-byte boundary. Fetch only ever sees the aligned bits (redirect_pc is
  //! [63:2]), so a misaligned target must fault here, before the low bits are dropped.
  logic pc_misaligned;

  assign pc_misaligned     = valid_op && ex_is_branch && branch_taken && |alu_result[1:0];
  assign wb_redirect_valid = completing && valid_op && ex_is_branch && branch_taken
                            && !pc_misaligned;
  assign wb_redirect_pc    = alu_result[63:2];

  // MUL/DIV decode as legal, but vea_alu has no silicon for them and raises
  // alu_unsupported instead. That must not reach the register file.
  assign wb_valid = completing && valid_op && ex_wr_en && !alu_unsupported;
  assign wb_rd    = ex_rd;
  assign wb_data  = ex_is_load ? load_value : alu_result;

  // ---- Status indicators ---------------------------------------------

  always_ff @(posedge clk) begin
    if (!rst_n) begin
      err_illegal     <= 1'b0;
      err_unsupported <= 1'b0;
      err_trap        <= 1'b0;
      err_unaligned   <= 1'b0;
    end else begin
      if (completing && ex_valid && ex_illegal)      err_illegal     <= 1'b1;
      if (completing && valid_op && alu_unsupported) err_unsupported <= 1'b1;
      if (completing && valid_op && ex_is_trap)      err_trap        <= 1'b1;
      if (completing && pc_misaligned)                err_unaligned   <= 1'b1;
    end
  end

endmodule
