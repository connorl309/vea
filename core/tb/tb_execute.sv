//! Testbench for vea_execute. See ../execute.sv for the module.

module tb_execute #(
  parameter int SEED = 0
);
  logic clk   = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  // Execute should not read ex_pc anywhere in the module. A fixed, odd value finds a bug
  // that reads it by mistake.
  logic [63:0] ex_pc = 64'hA5A5_A5A5_A5A5_A5A5;

  logic         ex_valid = 1'b0;
  logic [3:0]   ex_alu_op = '0;
  logic [63:0]  ex_a = '0;
  logic [63:0]  ex_b = '0;
  logic [63:0]  ex_c = '0;
  logic         ex_fwd_a = 1'b0;
  logic         ex_fwd_b = 1'b0;
  logic         ex_fwd_c = 1'b0;
  logic [4:0]   ex_rd = '0;
  logic         ex_wr_en = 1'b0;
  logic         ex_is_branch = 1'b0;
  logic [2:0]   ex_pred = '0;
  logic         ex_is_load = 1'b0;
  logic         ex_is_store = 1'b0;
  logic [1:0]   ex_mem_size = '0;
  logic         ex_mem_sext = 1'b0;
  logic         ex_is_trap = 1'b0;
  logic         ex_is_halt = 1'b0;
  logic         ex_illegal = 1'b0;
  logic         ex_ready;

  logic [63:0]  fwd_data = '0;
  logic         redirect_valid = 1'b0;

  logic         wb_valid;
  logic [4:0]   wb_rd;
  logic [63:0]  wb_data;
  logic         wb_redirect_valid;
  logic [63:2]  wb_redirect_pc;

  logic         err_illegal;
  logic         err_unsupported;
  logic         err_trap;
  logic         err_unaligned;

  logic              mem_req_valid;
  logic              mem_req_ready;
  vea_pkg::mem_req_t mem_req;
  logic              mem_rvalid = 1'b0;
  logic [63:0]       mem_rdata = '0;

  vea_execute dut (.*);

  int errors = 0;
  int checks = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_execute: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  task automatic do_reset();
    rst_n          = 1'b0;
    ex_valid       = 1'b0;
    redirect_valid = 1'b0;
    tick(3);
    rst_n = 1'b1;
  endtask

  // ---- Instruction driver --------------------------------------------------------------

  // One field per ex_ input, except ex_pc, which stays fixed for the whole run.
  typedef struct {
    logic [3:0]  alu_op;
    logic [63:0] a, b, c;
    logic        fwd_a, fwd_b, fwd_c;
    logic [4:0]  rd;
    logic        wr_en;
    logic        is_branch;
    logic [2:0]  pred;
    logic        is_load, is_store;
    logic [1:0]  mem_size;
    logic        mem_sext;
    logic        is_trap;
    logic        illegal;
  } insn_t;

  task automatic drive(input insn_t i);
    ex_valid     = 1'b1;
    ex_alu_op    = i.alu_op;
    ex_a         = i.a;
    ex_b         = i.b;
    ex_c         = i.c;
    ex_fwd_a     = i.fwd_a;
    ex_fwd_b     = i.fwd_b;
    ex_fwd_c     = i.fwd_c;
    ex_rd        = i.rd;
    ex_wr_en     = i.wr_en;
    ex_is_branch = i.is_branch;
    ex_pred      = i.pred;
    ex_is_load   = i.is_load;
    ex_is_store  = i.is_store;
    ex_mem_size  = i.mem_size;
    ex_mem_sext  = i.mem_sext;
    ex_is_trap   = i.is_trap;
    ex_illegal   = i.illegal;
  endtask

  // Execute must use only its own copy of an instruction after the ID/EX register takes
  // it. Random values on every ex_ input find a design that reads an input directly.
  task automatic scramble_inputs();
    ex_alu_op    = 4'($urandom);
    ex_a         = {$urandom, $urandom};
    ex_b         = {$urandom, $urandom};
    ex_c         = {$urandom, $urandom};
    ex_fwd_a     = 1'($urandom);
    ex_fwd_b     = 1'($urandom);
    ex_fwd_c     = 1'($urandom);
    ex_rd        = 5'($urandom);
    ex_wr_en     = 1'($urandom);
    ex_is_branch = 1'($urandom);
    ex_pred      = 3'($urandom);
    ex_is_load   = 1'($urandom);
    ex_is_store  = 1'($urandom);
    ex_mem_size  = 2'($urandom);
    ex_mem_sext  = 1'($urandom);
    ex_is_trap   = 1'($urandom);
    ex_illegal   = 1'($urandom);
  endtask

  // The ID/EX register takes an instruction on the clock edge. The register must be
  // free, or the instruction is lost. ex_valid then goes low, so the register takes only
  // this one instruction.
  task automatic issue(input insn_t i);
    drive(i);
    #1;
    check64(64'(ex_ready), 64'b1, "ID/EX register is free for the next instruction");
    tick();
    ex_valid = 1'b0;
    scramble_inputs();
  endtask

  // Waits for the instruction in the ID/EX register to complete. ex_ready is
  // combinational: a non-memory instruction can already be complete right after the edge
  // that took it. The initial #1 catches that case.
  task automatic wait_ready();
    int guard = 0;
    #1;
    while (!ex_ready && guard < 500) begin
      tick();
      guard++;
    end
    check64(64'(ex_ready), 64'b1, "ex_ready never asserted");
  endtask

  // ---- Reference: ALU write-back ---------------------------------------------------------

  localparam logic [3:0] OP_ADD = 4'h0, OP_SUB = 4'h1, OP_AND = 4'h2, OP_OR = 4'h3;
  localparam logic [3:0] OP_NOT = 4'h4, OP_XOR = 4'h5, OP_SHL = 4'h6, OP_SHR = 4'h7;
  localparam logic [3:0] OP_SAR = 4'h8, OP_MUL = 4'h9, OP_DIV = 4'hA;
  localparam logic [3:0] OP_CMP = 4'hB, OP_CMP_S = 4'hC;

  // DIV, and any op the ALU does not define, must raise unsupported and must not reach
  // the register file. MUL has real silicon (a plain 64x64 multiply, low bits kept).
  task automatic ref_alu(input logic [3:0] op, input logic [63:0] a, input logic [63:0] b,
                         output logic [63:0] result, output logic unsupp);
    result = '0;
    unsupp = 1'b0;
    case (op)
      OP_ADD:            result = a + b;
      OP_SUB:            result = a - b;
      OP_AND:            result = a & b;
      OP_OR:             result = a | b;
      OP_NOT:            result = ~a;
      OP_XOR:            result = a ^ b;
      OP_SHL:            result = a << b[5:0];
      OP_SHR:            result = a >> b[5:0];
      OP_SAR:            result = $signed(a) >>> b[5:0];
      OP_MUL:            result = a * b;
      OP_CMP, OP_CMP_S:  result = '0;
      default:           unsupp = 1'b1;
    endcase
  endtask

  // ---- Reference: branch predicate -------------------------------------------------------

  localparam logic [2:0] PRED_ALWAYS = 3'd0, PRED_EQ = 3'd1, PRED_NE = 3'd2, PRED_LT = 3'd3;
  localparam logic [2:0] PRED_GE = 3'd4, PRED_GT = 3'd5, PRED_LE = 3'd6;

  function automatic bit ref_taken(input logic [2:0] pred, input bit eq, input bit lt);
    case (pred)
      PRED_ALWAYS: ref_taken = 1'b1;
      PRED_EQ:     ref_taken = eq;
      PRED_NE:     ref_taken = ~eq;
      PRED_LT:     ref_taken = lt;
      PRED_GE:     ref_taken = ~lt;
      PRED_GT:     ref_taken = ~eq & ~lt;
      PRED_LE:     ref_taken = eq | lt;
      default:     ref_taken = 1'b0; // predicate 7; Decode never issues it for a branch
    endcase
  endfunction

  // ---- Memory model ----------------------------------------------------------------------

  localparam logic [1:0] SIZE_D = 2'b00, SIZE_B = 2'b01, SIZE_H = 2'b10, SIZE_W = 2'b11;

  // A byte-addressed store. Execute reads and writes the low bytes of a 64-bit word, so
  // the model does the same.
  localparam int DMEM_SIZE = 4096;
  logic [7:0] dmem [DMEM_SIZE];

  // Low when a test wants to hold off the memory, to check that Execute keeps its
  // request asserted.
  logic mem_gate = 1'b1;
  assign mem_req_ready = mem_gate;

  function automatic int size_bytes(input logic [1:0] sz);
    case (sz)
      SIZE_B:  size_bytes = 1;
      SIZE_H:  size_bytes = 2;
      SIZE_W:  size_bytes = 4;
      default: size_bytes = 8; // SIZE_D
    endcase
  endfunction

  function automatic logic [63:0] mem_read(input logic [63:0] addr, input logic [1:0] sz);
    logic [63:0] d;
    d = '0;
    for (int i = 0; i < size_bytes(sz); i++) d[8*i +: 8] = dmem[int'(addr) + i];
    mem_read = d;
  endfunction

  task automatic mem_write(input logic [63:0] addr, input logic [63:0] data, input logic [1:0] sz);
    for (int i = 0; i < size_bytes(sz); i++) dmem[int'(addr) + i] = data[8*i +: 8];
  endtask

  function automatic logic [63:0] low_bytes(input logic [63:0] v, input logic [1:0] sz);
    case (sz)
      SIZE_B:  low_bytes = {56'b0, v[7:0]};
      SIZE_H:  low_bytes = {48'b0, v[15:0]};
      SIZE_W:  low_bytes = {32'b0, v[31:0]};
      default: low_bytes = v; // SIZE_D
    endcase
  endfunction

  // The reply comes 1 to 3 cycles after the request, never in the same cycle. Execute
  // has only one request in flight at a time, so the model needs no queue.
  logic        m_busy = 1'b0;
  int          m_cnt;
  logic [63:0] m_addr_q;
  logic [1:0]  m_size_q;

  always @(posedge clk) begin
    mem_rvalid <= 1'b0;
    if (!rst_n) begin
      m_busy <= 1'b0;
    end else begin
      if (m_busy) begin
        if (m_cnt == 1) begin
          mem_rvalid <= 1'b1;
          mem_rdata  <= mem_read(m_addr_q, m_size_q);
          m_busy     <= 1'b0;
        end else begin
          m_cnt <= m_cnt - 1;
        end
      end
      if (mem_req_valid && mem_req_ready) begin
        if (mem_req.we) mem_write(mem_req.addr, mem_req.wdata, mem_req.size);
        m_addr_q <= mem_req.addr;
        m_size_q <= mem_req.size;
        m_cnt    <= $urandom_range(1, 3);
        m_busy   <= 1'b1;
      end
    end
  end

  // ---- Retire helpers ---------------------------------------------------------------------

  // Checks a non-branch instruction against the ALU and load reference models, then lets
  // the condition codes and the memory state register settle before the next issue.
  task automatic retire(input string name, input insn_t i);
    logic [63:0] alu_res;
    logic        alu_unsupp;
    logic [63:0] want_data;
    logic        want_wr;
    logic [63:0] a_eff, b_eff;

    issue(i);
    wait_ready();

    a_eff = i.fwd_a ? fwd_data : i.a;
    b_eff = i.fwd_b ? fwd_data : i.b;
    ref_alu(i.alu_op, a_eff, b_eff, alu_res, alu_unsupp);
    want_wr   = i.wr_en & ~alu_unsupp;
    want_data = i.is_load ? low_bytes(mem_rdata, i.mem_size) : alu_res;
    if (i.is_load && i.mem_sext) begin
      case (i.mem_size)
        SIZE_B:  want_data = {{56{mem_rdata[7]}},  mem_rdata[7:0]};
        SIZE_H:  want_data = {{48{mem_rdata[15]}}, mem_rdata[15:0]};
        SIZE_W:  want_data = {{32{mem_rdata[31]}}, mem_rdata[31:0]};
        default: want_data = mem_rdata; // SIZE_D has no sign to extend
      endcase
    end

    check64(64'(wb_valid), 64'(want_wr), {name, " wb_valid"});
    if (want_wr) begin
      check64(64'(wb_rd),   64'(i.rd),   {name, " wb_rd"});
      check64(wb_data,      want_data,   {name, " wb_data"});
    end
    check64(64'(wb_redirect_valid), 64'b0, {name, " wb_redirect_valid"});

    tick();
  endtask

  task automatic retire_branch(input string name, input insn_t i, input bit want_taken);
    logic [63:0] sum;

    issue(i);
    wait_ready();

    sum = (i.fwd_a ? fwd_data : i.a) + (i.fwd_b ? fwd_data : i.b);
    check64(64'(wb_redirect_valid), 64'(want_taken), {name, " wb_redirect_valid"});
    if (want_taken) check64(64'(wb_redirect_pc), 64'(sum[63:2]), {name, " wb_redirect_pc"});
    check64(64'(wb_valid), 64'b0, {name, " wb_valid"});

    tick();
  endtask

  // Issues a CMP or CMP_S so the next branch reads its condition codes. No output check:
  // the branch that follows is the real test of the result.
  task automatic issue_cmp(input logic [3:0] op, input logic [63:0] a, input logic [63:0] b);
    insn_t i;
    i        = '{default: '0};
    i.alu_op = op;
    i.a      = a;
    i.b      = b;
    issue(i);
    wait_ready();
    tick();
  endtask

  // ---- Tests --------------------------------------------------------------------------

  task automatic test_reset();
    do_reset();
    check64(64'(dut.cc),          64'd0, "cc after reset");
    check64(64'(int'(dut.state)), 64'd0, "state after reset (S_IDLE)");
    check64(64'(ex_ready),        64'd1, "ex_ready high when idle and ex_valid low");
    check64(64'(err_illegal),     64'd0, "err_illegal after reset");
    check64(64'(err_unsupported), 64'd0, "err_unsupported after reset");
    check64(64'(err_trap),        64'd0, "err_trap after reset");
    check64(64'(err_unaligned),   64'd0, "err_unaligned after reset");
  endtask

  // Every ALU op that does not stop the core, at directed and random operand values,
  // against ref_alu. Also checks that wr_en low suppresses the write even though the ALU
  // still runs. DIV has its own test, since it stops the core (test_unsupported_stops).
  task automatic test_alu_writeback();
    logic [3:0]  ops [10];
    logic [63:0] av, bv;
    insn_t       i;

    ops = '{OP_ADD, OP_SUB, OP_AND, OP_OR, OP_NOT, OP_XOR, OP_SHL, OP_SHR, OP_SAR, OP_MUL};

    for (int o = 0; o < 10; o++) begin
      for (int n = 0; n < 20; n++) begin
        av = (n == 0) ? 64'h0 : (n == 1) ? 64'hFFFF_FFFF_FFFF_FFFF : {$urandom, $urandom};
        bv = (n == 0) ? 64'h0 : {$urandom, $urandom};
        i        = '{default: '0};
        i.alu_op = ops[o];
        i.a      = av;
        i.b      = bv;
        i.wr_en  = 1'b1;
        i.rd     = 5'(n % 32);
        retire($sformatf("alu op %h n %0d", ops[o], n), i);
      end
    end

    i        = '{default: '0};
    i.alu_op = OP_ADD;
    i.a      = 64'd1;
    i.b      = 64'd1;
    i.wr_en  = 1'b0;
    i.rd     = 5'd7;
    retire("add with wr_en low", i);
  endtask

  // After a fault stops the core, Execute must never assert ex_ready again: drives a
  // harmless ADD and confirms it is refused for several cycles.
  task automatic check_stuck(input string name);
    insn_t i;
    i        = '{default: '0};
    i.alu_op = OP_ADD;
    i.a      = 64'd1;
    i.b      = 64'd1;
    drive(i);
    for (int c = 0; c < 5; c++) begin
      tick();
      check64(64'(ex_ready), 64'b0, {name, ": ex_ready stays low once the core has stopped"});
    end
    ex_valid = 1'b0;
  endtask

  // ex_illegal must suppress the write and the redirect, even when the other fields ask
  // for both, and it must stop the core: nothing after it may ever retire.
  task automatic test_illegal();
    insn_t i;
    i           = '{default: '0};
    i.alu_op    = OP_ADD;
    i.a         = 64'hDEAD;
    i.b         = 64'hBEEF;
    i.wr_en     = 1'b1;
    i.rd        = 5'd9;
    i.is_branch = 1'b1;
    i.pred      = PRED_ALWAYS;
    i.illegal   = 1'b1;

    issue(i);
    wait_ready();
    check64(64'(wb_valid),          64'b0, "illegal instruction: no write");
    check64(64'(wb_redirect_valid), 64'b0, "illegal instruction: no redirect");
    tick();
    check64(64'(err_illegal), 64'b1, "err_illegal latches after an illegal instruction");
    check_stuck("illegal");
    do_reset();
  endtask

  // DIV is a legal opcode with no silicon: it must raise alu_unsupported, suppress the
  // write, and stop the core the same way an illegal instruction does.
  task automatic test_unsupported_stops();
    insn_t i;
    i        = '{default: '0};
    i.alu_op = OP_DIV;
    i.a      = 64'd10;
    i.b      = 64'd3;
    i.wr_en  = 1'b1;
    i.rd     = 5'd4;

    issue(i);
    wait_ready();
    check64(64'(wb_valid), 64'b0, "div: no write");
    tick();
    check64(64'(err_unsupported), 64'b1, "err_unsupported latches after div");
    check_stuck("unsupported");
    do_reset();
  endtask

  // TRAP has no vector yet: it must retire with no write and no redirect, latch
  // err_trap, and stop the core, the same as illegal and unsupported.
  task automatic test_trap_stops();
    insn_t i;
    i         = '{default: '0};
    i.alu_op  = OP_ADD;
    i.a       = 64'h5;
    i.is_trap = 1'b1;

    issue(i);
    wait_ready();
    check64(64'(wb_valid),          64'b0, "trap: no write");
    check64(64'(wb_redirect_valid), 64'b0, "trap: no redirect");
    tick();
    check64(64'(err_trap), 64'b1, "err_trap latches after a trap");
    check_stuck("trap");
    do_reset();
  endtask

  // A CMP or CMP_S, then every predicate. eq and lt come from the operands directly, not
  // from the condition-code bits, so this finds a wrong bit mapping as well as a wrong
  // predicate case.
  task automatic test_branch();
    logic [63:0] va [10];
    logic [63:0] vb [10];
    bit          eq, lt_u, lt_s;
    insn_t       bi;

    va[0] = 64'd5;                     vb[0] = 64'd5;
    va[1] = 64'd5;                     vb[1] = 64'd9;
    va[2] = 64'd9;                     vb[2] = 64'd5;
    va[3] = 64'h8000_0000_0000_0000;   vb[3] = 64'd1;
    va[4] = 64'd1;                     vb[4] = 64'h8000_0000_0000_0000;
    va[5] = -64'sd1;                   vb[5] = 64'd0;
    va[6] = 64'd0;                     vb[6] = -64'sd1;
    va[7] = 64'hFFFF_FFFF_FFFF_FFFF;   vb[7] = 64'hFFFF_FFFF_FFFF_FFFE;
    va[8] = {$urandom, $urandom};      vb[8] = {$urandom, $urandom};
    va[9] = va[8];                     vb[9] = va[8];

    for (int k = 0; k < 10; k++) begin
      eq   = (va[k] == vb[k]);
      lt_u = (va[k] <  vb[k]);
      lt_s = ($signed(va[k]) < $signed(vb[k]));

      issue_cmp(OP_CMP, va[k], vb[k]);
      for (int p = 0; p <= 7; p++) begin
        bi           = '{default: '0};
        bi.alu_op    = OP_ADD;
        bi.a         = {$urandom, $urandom} & ~64'h3;
        bi.is_branch = 1'b1;
        bi.pred      = 3'(p);
        retire_branch($sformatf("cmp %0d/%0d pred %0d", k, p, p), bi, ref_taken(3'(p), eq, lt_u));
      end

      issue_cmp(OP_CMP_S, va[k], vb[k]);
      for (int p = 0; p <= 7; p++) begin
        bi           = '{default: '0};
        bi.alu_op    = OP_ADD;
        bi.a         = {$urandom, $urandom} & ~64'h3;
        bi.is_branch = 1'b1;
        bi.pred      = 3'(p);
        retire_branch($sformatf("cmp.s %0d/%0d pred %0d", k, p, p), bi, ref_taken(3'(p), eq, lt_s));
      end
    end
  endtask

  // A taken branch or jump whose target lands off a 4-byte boundary must not redirect,
  // and must latch err_unaligned. One case per low-bit pattern, plus one aligned control
  // case that must redirect and must not latch it.
  task automatic test_unaligned_branch();
    insn_t bi;

    for (int off = 0; off < 4; off++) begin
      bi           = '{default: '0};
      bi.alu_op    = OP_ADD;
      bi.a         = (64'h1000 + 64'(off)) & ~64'h3;
      bi.b         = 64'(off);
      bi.is_branch = 1'b1;
      bi.pred      = PRED_ALWAYS;
      issue(bi);
      wait_ready();
      check64(64'(wb_redirect_valid), 64'(off == 0),
              $sformatf("unaligned +%0d: wb_redirect_valid", off));
      tick();
      check64(64'(err_unaligned), 64'(off != 0), $sformatf("unaligned +%0d: err_unaligned", off));
      if (off != 0) begin
        check_stuck($sformatf("unaligned +%0d", off));
        do_reset(); // clear the latch so the next offset starts clean
      end
    end
  endtask

  // The condition codes must hold across an instruction that does not write them.
  task automatic test_cc_persistence();
    insn_t ai, bi;

    issue_cmp(OP_CMP, 64'd3, 64'd9); // a < b: LT must take, GE must not

    ai        = '{default: '0};
    ai.alu_op = OP_ADD;
    ai.a      = 64'd1;
    ai.b      = 64'd1;
    ai.wr_en  = 1'b1;
    ai.rd     = 5'd2;
    retire("intervening add", ai);

    bi           = '{default: '0};
    bi.alu_op    = OP_ADD;
    bi.a         = 64'h100;
    bi.is_branch = 1'b1;
    bi.pred      = PRED_LT;
    retire_branch("branch after intervening add", bi, 1'b1);
  endtask

  // A load at every size and sign-extend combination, against known memory content.
  task automatic test_load();
    insn_t       i;
    logic [1:0]  sizes [4];
    logic [63:0] addr, data;

    sizes = '{SIZE_B, SIZE_H, SIZE_W, SIZE_D};

    for (int s = 0; s < 4; s++) begin
      for (int sx = 0; sx < 2; sx++) begin
        for (int n = 0; n < 5; n++) begin
          addr = 64'(n * 16 + s * 64);
          data = {$urandom, $urandom};
          mem_write(addr, data, sizes[s]);

          i          = '{default: '0};
          i.alu_op   = OP_ADD;
          i.a        = addr;
          i.is_load  = 1'b1;
          i.wr_en    = 1'b1;
          i.rd       = 5'(n + 1);
          i.mem_size = sizes[s];
          i.mem_sext = 1'(sx);
          retire($sformatf("load size %0d sext %0d n %0d", s, sx, n), i);
        end
      end
    end
  endtask

  // A store at every size, then a direct read of the memory model to check the bytes it
  // wrote and the address and size it used.
  task automatic test_store();
    insn_t       i;
    logic [1:0]  sizes [4];
    logic [63:0] addr, value, readback;

    sizes = '{SIZE_B, SIZE_H, SIZE_W, SIZE_D};

    for (int s = 0; s < 4; s++) begin
      for (int n = 0; n < 5; n++) begin
        addr  = 64'(n * 16 + s * 64 + 2048);
        value = {$urandom, $urandom};

        i          = '{default: '0};
        i.alu_op   = OP_ADD;
        i.a        = addr;
        i.c        = value;
        i.is_store = 1'b1;
        i.mem_size = sizes[s];
        retire($sformatf("store size %0d n %0d", s, n), i);

        readback = mem_read(addr, sizes[s]);
        check64(readback, low_bytes(value, sizes[s]),
                $sformatf("store size %0d n %0d: memory content", s, n));
      end
    end
  endtask

  // With the memory not ready, Execute must hold its request and must not signal ready
  // until the memory both accepts the request and replies.
  task automatic test_mem_backpressure();
    insn_t       i;
    logic [63:0] addr;

    addr = 64'd3000;
    mem_write(addr, 64'hCAFE_BABE_DEAD_BEEF, SIZE_D);

    mem_gate   = 1'b0;
    i          = '{default: '0};
    i.alu_op   = OP_ADD;
    i.a        = addr;
    i.is_load  = 1'b1;
    i.wr_en    = 1'b1;
    i.rd       = 5'd11;
    i.mem_size = SIZE_D;
    issue(i);
    check64(64'(mem_req_valid), 64'b0, "no request in the first cycle: the address is not ready");

    for (int c = 0; c < 5; c++) begin
      tick();
      check64(64'(mem_req_valid), 64'b1, "mem_req_valid must hold while the memory is not ready");
      check64(mem_req.addr,       addr,  "mem_req.addr must hold while the memory is not ready");
      check64(64'(ex_ready),      64'b0, "ex_ready must stay low while the request is not accepted");
    end
    mem_gate = 1'b1;

    wait_ready();
    check64(wb_data, 64'hCAFE_BABE_DEAD_BEEF, "load after backpressure: data");
    tick();
  endtask

  // ---- ID/EX register and forwarding ------------------------------------------------------

  // The ID/EX register hides an instruction from the outputs until the clock edge takes
  // it. An instruction that reaches wb_valid in the cycle it is presented puts the
  // register file read and the ALU back into one cycle. OR is combinational, so it
  // completes in the first cycle after the edge. ADD and SUB wait one more cycle for their
  // adder.
  task automatic test_id_ex_register();
    insn_t i;

    i        = '{default: '0};
    i.alu_op = OP_OR;
    i.a      = 64'd1;
    i.b      = 64'd2;
    i.wr_en  = 1'b1;
    i.rd     = 5'd3;
    drive(i);
    #1;
    check64(64'(wb_valid), 64'b0, "presented instruction: wb_valid low before the edge");
    tick();
    ex_valid = 1'b0;
    check64(64'(wb_valid), 64'b1, "wb_valid high after the edge takes the instruction");
    check64(wb_data,       64'd3, "wb_data after the edge takes the instruction");
    tick();
    check64(64'(wb_valid), 64'b0, "wb_valid low once the register is empty");
  endtask

  // ADD and SUB take two cycles in Execute: the first cycle loads the adder register,
  // and the second cycle has the result. The register must stay busy in between, and
  // wb_valid must pulse only in the second cycle.
  task automatic test_add_wait();
    logic [3:0] ops [2];
    logic [63:0] want;
    insn_t i;

    ops = '{OP_ADD, OP_SUB};
    for (int o = 0; o < 2; o++) begin
      i        = '{default: '0};
      i.alu_op = ops[o];
      i.a      = 64'd100;
      i.b      = 64'd58;
      i.wr_en  = 1'b1;
      i.rd     = 5'd3;
      want     = (o == 0) ? 64'd158 : 64'd42;

      drive(i);
      #1;
      check64(64'(wb_valid), 64'b0, $sformatf("op %0d: wb_valid low before the edge", o));
      tick();
      ex_valid = 1'b0;
      scramble_inputs();
      #1;
      check64(64'(wb_valid), 64'b0, $sformatf("op %0d: wb_valid low in the first cycle", o));
      check64(64'(ex_ready), 64'b0, $sformatf("op %0d: register busy in the first cycle", o));
      tick();
      check64(64'(wb_valid), 64'b1, $sformatf("op %0d: wb_valid high in the second cycle", o));
      check64(wb_data,       want,  $sformatf("op %0d: result in the second cycle", o));
      check64(64'(ex_ready), 64'b1, $sformatf("op %0d: register free in the second cycle", o));
      tick();
      check64(64'(wb_valid), 64'b0, $sformatf("op %0d: wb_valid low in the third cycle", o));
    end
  endtask

  // An instruction that completes in one cycle must free the register for the next one
  // in that same cycle. Without this, the core issues an instruction every second cycle.
  // OR is combinational, so this checks the one-cycle case. ADD has its own test below.
  task automatic test_back_to_back();
    insn_t a, b;

    a        = '{default: '0};
    a.alu_op = OP_OR;
    a.a      = 64'd1;
    a.b      = 64'd2;
    a.wr_en  = 1'b1;
    a.rd     = 5'd3;
    b        = '{default: '0};
    b.alu_op = OP_OR;
    b.a      = 64'd10;
    b.b      = 64'd20;
    b.wr_en  = 1'b1;
    b.rd     = 5'd4;

    drive(a);
    tick();
    drive(b);
    #1;
    check64(64'(ex_ready), 64'b1, "back to back: ex_ready while the first instruction completes");
    check64(wb_data,       64'd3, "back to back: first result");
    tick();
    ex_valid = 1'b0;
    check64(64'(wb_rd), 64'd4,  "back to back: second rd");
    check64(wb_data,    64'd30, "back to back: second result");
    tick();
  endtask

  // Two ADDs in a row. The second one must wait for the first, and each result must come
  // in the second cycle of its own instruction, not the first result again.
  task automatic test_add_back_to_back();
    insn_t a, b;

    a        = '{default: '0};
    a.alu_op = OP_ADD;
    a.a      = 64'd1;
    a.b      = 64'd2;
    a.wr_en  = 1'b1;
    a.rd     = 5'd3;
    b        = '{default: '0};
    b.alu_op = OP_ADD;
    b.a      = 64'd10;
    b.b      = 64'd20;
    b.wr_en  = 1'b1;
    b.rd     = 5'd4;

    drive(a);
    tick();
    drive(b);
    #1;
    check64(64'(ex_ready), 64'b0, "add back to back: ex_ready low while the first add waits");
    check64(64'(wb_valid), 64'b0, "add back to back: no write in the first cycle");
    tick();
    check64(64'(ex_ready), 64'b1, "add back to back: ex_ready while the first add completes");
    check64(64'(wb_rd),    64'd3, "add back to back: first rd");
    check64(wb_data,       64'd3, "add back to back: first result");
    tick();
    ex_valid = 1'b0;
    check64(64'(wb_valid), 64'b0, "add back to back: second add waits, no write");
    tick();
    check64(64'(wb_rd), 64'd4,  "add back to back: second rd");
    check64(wb_data,    64'd30, "add back to back: second result");
    tick();
  endtask

  // A forwarded operand comes from fwd_data, not from the ex_ input that the ID/EX
  // register took. The stale ex_ value is random, so a design that ignores the select
  // gets a wrong result.
  task automatic test_forward();
    logic [3:0] ops [3];
    insn_t      i;

    ops = '{OP_ADD, OP_SUB, OP_MUL};
    for (int o = 0; o < 3; o++)
      for (int n = 0; n < 8; n++)
        for (int f = 0; f < 4; f++) begin
          fwd_data = {$urandom, $urandom};
          i        = '{default: '0};
          i.alu_op = ops[o];
          i.a      = {$urandom, $urandom};
          i.b      = {$urandom, $urandom};
          i.fwd_a  = f[0];
          i.fwd_b  = f[1];
          i.wr_en  = 1'b1;
          i.rd     = 5'(n + 1);
          retire($sformatf("forward op %h n %0d select %0d", ops[o], n, f), i);
        end
    fwd_data = '0;
  endtask

  // The target of a branch is the ALU sum, so a forwarded operand must reach the target.
  task automatic test_forward_branch();
    insn_t i;

    for (int f = 0; f < 4; f++) begin
      fwd_data     = {$urandom, $urandom} & ~64'h3;
      i            = '{default: '0};
      i.alu_op     = OP_ADD;
      i.a          = {$urandom, $urandom} & ~64'h3;
      i.b          = {$urandom, $urandom} & ~64'h3;
      i.fwd_a      = f[0];
      i.fwd_b      = f[1];
      i.is_branch  = 1'b1;
      i.pred       = PRED_ALWAYS;
      retire_branch($sformatf("forward branch select %0d", f), i, 1'b1);
    end
    fwd_data = '0;
  endtask

  // The address of a load and the value of a store are both forwarded operands. The
  // address is checked on the memory port, because the load result alone does not show
  // a wrong address.
  task automatic test_forward_mem();
    insn_t       i;
    logic [1:0]  sizes [4];
    logic [63:0] addr, data;

    sizes = '{SIZE_B, SIZE_H, SIZE_W, SIZE_D};

    for (int s = 0; s < 4; s++) begin
      addr = 64'(3200 + s * 16);
      data = {$urandom, $urandom};
      mem_write(addr, data, sizes[s]);

      fwd_data   = addr;
      i          = '{default: '0};
      i.alu_op   = OP_ADD;
      i.a        = {$urandom, $urandom};
      i.fwd_a    = 1'b1;
      i.is_load  = 1'b1;
      i.wr_en    = 1'b1;
      i.rd       = 5'd6;
      i.mem_size = sizes[s];
      issue(i);
      tick();
      check64(64'(mem_req_valid), 64'b1, $sformatf("forwarded load request, size %0d", s));
      check64(mem_req.addr, addr, $sformatf("forwarded load address, size %0d", s));
      wait_ready();
      check64(wb_data, low_bytes(data, sizes[s]), $sformatf("forwarded load data, size %0d", s));
      tick();

      addr       = 64'(3300 + s * 16);
      data       = {$urandom, $urandom};
      fwd_data   = data;
      i          = '{default: '0};
      i.alu_op   = OP_ADD;
      i.a        = addr;
      i.c        = {$urandom, $urandom};
      i.fwd_c    = 1'b1;
      i.is_store = 1'b1;
      i.mem_size = sizes[s];
      retire($sformatf("forwarded store, size %0d", s), i);
      check64(mem_read(addr, sizes[s]), low_bytes(data, sizes[s]),
              $sformatf("forwarded store value, size %0d", s));
    end
    fwd_data = '0;
  endtask

  // A redirect arrives in the cycle after the branch completes. The instruction that
  // entered the ID/EX register in that cycle is on the wrong path, and it must change
  // nothing. Each case here has one side effect that must not happen.
  task automatic test_redirect_squash();
    insn_t i, bi;

    i        = '{default: '0};
    i.alu_op = OP_ADD;
    i.a      = 64'd1;
    i.b      = 64'd2;
    i.wr_en  = 1'b1;
    i.rd     = 5'd9;
    issue(i);
    redirect_valid = 1'b1;
    #1;
    check64(64'(wb_valid), 64'b0, "wrong-path write: no wb_valid");
    check64(64'(ex_ready), 64'b1, "wrong-path add: it does not wait for the adder");
    tick();
    redirect_valid = 1'b0;
    check64(64'(wb_valid), 64'b0, "wrong-path write: no wb_valid after the redirect");
    check64(64'(ex_ready), 64'b1, "wrong-path write: ID/EX register empty after the redirect");

    // The condition codes keep the result of the CMP before the wrong-path CMP.
    issue_cmp(OP_CMP, 64'd5, 64'd5);
    i        = '{default: '0};
    i.alu_op = OP_CMP;
    i.a      = 64'd1;
    i.b      = 64'd9;
    issue(i);
    redirect_valid = 1'b1;
    tick();
    redirect_valid = 1'b0;
    bi           = '{default: '0};
    bi.alu_op    = OP_ADD;
    bi.a         = 64'h100;
    bi.is_branch = 1'b1;
    bi.pred      = PRED_EQ;
    retire_branch("wrong-path cmp does not change the condition codes", bi, 1'b1);

    i          = '{default: '0};
    i.alu_op   = OP_ADD;
    i.a        = 64'd3000;
    i.is_load  = 1'b1;
    i.wr_en    = 1'b1;
    i.rd       = 5'd2;
    i.mem_size = SIZE_D;
    issue(i);
    redirect_valid = 1'b1;
    #1;
    check64(64'(mem_req_valid), 64'b0, "wrong-path load: no memory request");
    tick();
    redirect_valid = 1'b0;
    check64(64'(mem_req_valid), 64'b0, "wrong-path load: no memory request after the redirect");

    // A taken branch with a target off a 4-byte boundary would fault, and would redirect.
    bi           = '{default: '0};
    bi.alu_op    = OP_ADD;
    bi.a         = 64'h1001;
    bi.is_branch = 1'b1;
    bi.pred      = PRED_ALWAYS;
    issue(bi);
    redirect_valid = 1'b1;
    #1;
    check64(64'(wb_redirect_valid), 64'b0, "wrong-path branch: no redirect");
    tick();
    redirect_valid = 1'b0;
    check64(64'(err_unaligned), 64'b0, "wrong-path branch: no err_unaligned");

    i         = '{default: '0};
    i.illegal = 1'b1;
    issue(i);
    redirect_valid = 1'b1;
    tick();
    redirect_valid = 1'b0;
    check64(64'(err_illegal), 64'b0, "wrong-path illegal instruction: no err_illegal");
    check64(64'(ex_ready),    64'b1, "wrong-path illegal instruction: the core does not stop");
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_execute: seed %0d", SEED);
    $display("tb_execute: test_reset");
    test_reset();
    $display("tb_execute: test_alu_writeback");
    test_alu_writeback();
    $display("tb_execute: test_illegal");
    test_illegal();
    $display("tb_execute: test_unsupported_stops");
    test_unsupported_stops();
    $display("tb_execute: test_trap_stops");
    test_trap_stops();
    $display("tb_execute: test_branch");
    test_branch();
    $display("tb_execute: test_unaligned_branch");
    test_unaligned_branch();
    $display("tb_execute: test_cc_persistence");
    test_cc_persistence();
    $display("tb_execute: test_load");
    test_load();
    $display("tb_execute: test_store");
    test_store();
    $display("tb_execute: test_mem_backpressure");
    test_mem_backpressure();
    $display("tb_execute: test_id_ex_register");
    test_id_ex_register();
    $display("tb_execute: test_add_wait");
    test_add_wait();
    $display("tb_execute: test_back_to_back");
    test_back_to_back();
    $display("tb_execute: test_add_back_to_back");
    test_add_back_to_back();
    $display("tb_execute: test_forward");
    test_forward();
    $display("tb_execute: test_forward_branch");
    test_forward_branch();
    $display("tb_execute: test_forward_mem");
    test_forward_mem();
    $display("tb_execute: test_redirect_squash");
    test_redirect_squash();

    $display("tb_execute: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_execute failed");
    $finish;
  end

  initial begin
    #2000000;
    $fatal(1, "tb_execute watchdog");
  end
endmodule
