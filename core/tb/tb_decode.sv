//! Testbench for vea_decode.
//!
//! The model in this file builds each frame as the assembler does.

module tb_decode;
  localparam int MB = 13;
  localparam int NV = 100;

  logic clk   = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  logic            frame_valid    = 1'b0;
  logic            frame_ready;
  logic [63:0]     frame_pc       = '0;
  logic [8*MB-1:0] frame_bytes    = '0;
  logic            frame_bad_len  = 1'b0;
  logic            redirect_valid = 1'b0;
  logic            halt;
  logic [4:0]      rf_raddr_a, rf_raddr_b, rf_raddr_c;
  logic [63:0]     rf_rdata_a, rf_rdata_b, rf_rdata_c;
  logic            ex_valid;
  logic            ex_ready       = 1'b1;
  logic [63:0]     ex_pc;
  logic [3:0]      ex_alu_op;
  logic [63:0]     ex_a, ex_b, ex_c;
  logic [4:0]      ex_rd;
  logic            ex_wr_en, ex_is_branch;
  logic [2:0]      ex_pred;
  logic            ex_is_load, ex_is_store;
  logic [1:0]      ex_mem_size;
  logic            ex_mem_sext, ex_is_trap, ex_is_halt, ex_illegal;

  vea_decode #(.MAX_INSN_BYTES(MB)) dut (.*);

  // Every register has its own value, so a wrong register number shows in the data.
  logic [63:0] rf [32];
  assign rf_rdata_a = rf[rf_raddr_a];
  assign rf_rdata_b = rf[rf_raddr_b];
  assign rf_rdata_c = rf[rf_raddr_c];

  int errors = 0;
  int checks = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_decode: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  // ---- Reference model -------------------------------------------------------------

  logic [7:0] legal_ops [21];

  function automatic bit ref_legal(input logic [7:0] op);
    ref_legal = 1'b0;
    for (int i = 0; i < 21; i++) if (legal_ops[i] == op) ref_legal = 1'b1;
  endfunction

  // The byte where the last operand starts. NOP and HALT have no operand.
  function automatic int ref_head(input logic [7:0] op);
    case (op)
      8'h01, 8'h14, 8'h20, 8'h21: ref_head = 3;
      8'h30, 8'h31, 8'hFE:        ref_head = 2;
      8'h00, 8'hFF:               ref_head = 0;
      default:                    ref_head = 4;
    endcase
  endfunction

  function automatic bit ref_imm_form(input logic [7:0] op, input logic flag3, input logic flag0);
    case (op)
      8'h30, 8'h31:  ref_imm_form = flag3;
      8'hFE:         ref_imm_form = 1'b1;
      8'h00, 8'hFF:  ref_imm_form = 1'b0;
      default:       ref_imm_form = flag0;
    endcase
  endfunction

  function automatic bit ref_reg_used(input logic [7:0] op, input bit imm, input int pos);
    case (ref_head(op))
      2:       ref_reg_used = (pos == 2) && !imm;
      3:       ref_reg_used = (pos == 2) || (pos == 3 && !imm);
      4:       ref_reg_used = (pos == 2) || (pos == 3) || (pos == 4 && !imm);
      default: ref_reg_used = 1'b0;
    endcase
  endfunction

  // The rules are the ones of the simulator: opcode, register number, immediate of
  // more than 8 bytes, and predicate. The length rules come from Fetch and the ISA.
  function automatic bit ref_illegal(input logic [7:0] op, input logic [3:0] flags,
                                     input int len, input logic [7:0] b2,
                                     input logic [7:0] b3, input logic [7:0] b4);
    bit imm;
    imm = ref_imm_form(op, flags[3], flags[0]);
    ref_illegal = !ref_legal(op) || len < 2 || len > 12;
    if (!ref_illegal && imm && (len - ref_head(op)) > 8) ref_illegal = 1'b1;
    if (!ref_illegal && op == 8'h30 && flags[2:0] == 3'b111) ref_illegal = 1'b1;
    if (!ref_illegal && ref_reg_used(op, imm, 2) && b2 >= 8'd32) ref_illegal = 1'b1;
    if (!ref_illegal && ref_reg_used(op, imm, 3) && b3 >= 8'd32) ref_illegal = 1'b1;
    if (!ref_illegal && ref_reg_used(op, imm, 4) && b4 >= 8'd32) ref_illegal = 1'b1;
  endfunction

  // The shortest signed width that gives the value back, as the assembler does. The
  // value zero has no bytes.
  function automatic int imm_width(input logic signed [63:0] v);
    imm_width = 8;
    if (v == 0) imm_width = 0;
    else
      for (int w = 7; w >= 1; w--)
        if (((v <<< (64 - 8 * w)) >>> (64 - 8 * w)) == v) imm_width = w;
  endfunction

  // ---- Frame builder ---------------------------------------------------------------

  logic [8*MB-1:0] fbv;
  int              pos;
  // The bytes after the instruction belong to the next instruction. Decode must ignore
  // them, and a sign bit in them must not reach the immediate.
  int          junk_mode = 0;
  logic [63:0] pc_val    = 64'h0000_0000_1234_5670;

  function automatic logic [7:0] junk_byte();
    case (junk_mode)
      1:       junk_byte = 8'hFF;
      2:       junk_byte = 8'h00;
      default: junk_byte = 8'($urandom);
    endcase
  endfunction

  function automatic logic [8*MB-1:0] junk_frame();
    logic [8*MB-1:0] r;
    for (int i = 0; i < MB; i++) r[8*i +: 8] = junk_byte();
    junk_frame = r;
  endfunction

  // Byte 0 is the opcode, at the top of the vector.
  function automatic logic [8*MB-1:0] with_byte(input logic [8*MB-1:0] v, input int idx,
                                                input logic [7:0] b);
    logic [8*MB-1:0] r;
    r = v;
    r[8*(MB-1-idx) +: 8] = b;
    with_byte = r;
  endfunction

  task automatic begin_insn(input logic [7:0] opcode);
    fbv = with_byte(junk_frame(), 0, opcode);
    pos = 2;
  endtask

  // The random sweep tests the register numbers that are not defined. A directed test
  // that builds one has an error in the test.
  task automatic put_reg(input int r);
    check64(64'(r >= 0 && r < 32), 64'b1, "put_reg: register number is not defined");
    fbv = with_byte(fbv, pos, 8'(r));
    pos++;
  endtask

  task automatic put_imm(input logic signed [63:0] v);
    int w;
    w = imm_width(v);
    for (int i = w - 1; i >= 0; i--) begin
      fbv = with_byte(fbv, pos, v[8*i +: 8]);
      pos++;
    end
  endtask

  task automatic pack_frame(input int len, input logic [3:0] flags);
    fbv           = with_byte(fbv, 1, {4'(len), flags});
    frame_bytes   = fbv;
    frame_bad_len = (len < 2);
    frame_pc      = pc_val;
  endtask

  task automatic end_insn(input logic [3:0] flags);
    pack_frame(pos, flags);
  endtask

  // ---- Expected outputs of one instruction -----------------------------------------

  typedef struct {
    logic [3:0]  alu_op;
    logic [63:0] a, b, c;
    logic [4:0]  rd;
    logic        wr_en, is_branch, is_load, is_store, is_trap, is_halt;
    logic [2:0]  pred;
    logic [1:0]  mem_size;
    logic        mem_sext;
    // Some outputs have no meaning for some instructions. The test checks only the
    // outputs that the instruction defines.
    bit          chk_alu, chk_a, chk_b, chk_c, chk_rd, chk_pred, chk_mem;
  } exp_t;

  task automatic apply_check(input string name, input exp_t e);
    frame_valid = 1'b1;
    #1;
    check64(ex_pc,             frame_pc,              {name, " pc"});
    check64(64'(ex_illegal),   64'b0,                 {name, " illegal"});
    check64(64'(ex_wr_en),     64'(e.wr_en),          {name, " wr_en"});
    check64(64'(ex_is_branch), 64'(e.is_branch),      {name, " is_branch"});
    check64(64'(ex_is_load),   64'(e.is_load),        {name, " is_load"});
    check64(64'(ex_is_store),  64'(e.is_store),       {name, " is_store"});
    check64(64'(ex_is_trap),   64'(e.is_trap),        {name, " is_trap"});
    check64(64'(ex_is_halt),   64'(e.is_halt),        {name, " is_halt"});
    if (e.chk_alu)  check64(64'(ex_alu_op), 64'(e.alu_op), {name, " alu_op"});
    if (e.chk_a)    check64(ex_a, e.a, {name, " a"});
    if (e.chk_b)    check64(ex_b, e.b, {name, " b"});
    if (e.chk_c)    check64(ex_c, e.c, {name, " c"});
    if (e.chk_rd)   check64(64'(ex_rd), 64'(e.rd), {name, " rd"});
    if (e.chk_pred) check64(64'(ex_pred), 64'(e.pred), {name, " pred"});
    if (e.chk_mem) begin
      check64(64'(ex_mem_size), 64'(e.mem_size), {name, " mem_size"});
      check64(64'(ex_mem_sext), 64'(e.mem_sext), {name, " mem_sext"});
    end
  endtask

  // ---- Directed tests, one task for each operand format ------------------------------

  // mov and not: a destination, then a register or an immediate.
  task automatic t_unary(input logic [7:0] op, input int rd, input int rs, input bit use_imm,
                         input logic signed [63:0] v);
    exp_t e;
    begin_insn(op);
    put_reg(rd);
    if (use_imm) put_imm(v); else put_reg(rs);
    end_insn({3'b000, use_imm});
    e = '{default: '0};
    e.chk_alu = 1'b1; e.chk_a = 1'b1; e.chk_b = 1'b1; e.chk_rd = 1'b1;
    e.alu_op = (op == 8'h14) ? 4'h4 : 4'h0;
    e.a      = use_imm ? 64'(v) : rf[rs];
    e.rd     = 5'(rd);
    e.wr_en  = 1'b1;
    apply_check($sformatf("op %h rd %0d rs %0d imm %0d v %0d jm %0d", op, rd, rs, use_imm, v, junk_mode), e);
  endtask

  // cmp and cmp.s: a register, then a register or an immediate.
  task automatic t_cmp(input logic [7:0] op, input int ra, input int rs, input bit use_imm,
                       input logic signed [63:0] v);
    exp_t e;
    begin_insn(op);
    put_reg(ra);
    if (use_imm) put_imm(v); else put_reg(rs);
    end_insn({3'b000, use_imm});
    e = '{default: '0};
    e.chk_alu = 1'b1; e.chk_a = 1'b1; e.chk_b = 1'b1;
    e.alu_op = op[0] ? 4'hC : 4'hB;
    e.a      = rf[ra];
    e.b      = use_imm ? 64'(v) : rf[rs];
    apply_check($sformatf("op %h ra %0d rs %0d imm %0d v %0d jm %0d", op, ra, rs, use_imm, v, junk_mode), e);
  endtask

  // add to div, without not.
  task automatic t_alu(input logic [7:0] op, input int rd, input int rs1, input int rs2,
                       input bit use_imm, input logic signed [63:0] v);
    exp_t e;
    begin_insn(op);
    put_reg(rd);
    put_reg(rs1);
    if (use_imm) put_imm(v); else put_reg(rs2);
    end_insn({3'b000, use_imm});
    e = '{default: '0};
    e.chk_alu = 1'b1; e.chk_a = 1'b1; e.chk_b = 1'b1; e.chk_rd = 1'b1;
    e.alu_op = op[3:0];
    e.a      = rf[rs1];
    e.b      = use_imm ? 64'(v) : rf[rs2];
    e.rd     = 5'(rd);
    e.wr_en  = 1'b1;
    apply_check($sformatf("op %h rd %0d rs1 %0d rs2 %0d imm %0d v %0d jm %0d", op, rd, rs1, rs2, use_imm, v, junk_mode), e);
  endtask

  // b and jmp: one target, a register or an immediate.
  task automatic t_branch(input logic [7:0] op, input logic [2:0] pred, input bit use_imm,
                          input int rt, input logic signed [63:0] v);
    exp_t e;
    begin_insn(op);
    if (use_imm) put_imm(v); else put_reg(rt);
    end_insn({use_imm, pred});
    e = '{default: '0};
    e.chk_alu = 1'b1; e.chk_a = 1'b1; e.chk_b = 1'b1; e.chk_pred = 1'b1;
    e.is_branch = 1'b1;
    e.alu_op    = 4'h0;
    // A jmp ignores its predicate bits.
    e.pred      = (op == 8'h30) ? pred : 3'b000;
    if (op == 8'h30) begin
      e.a = use_imm ? pc_val : rf[rt];
      e.b = use_imm ? 64'(v) : 64'b0;
    end else begin
      e.a = use_imm ? 64'(v) : rf[rt];
      e.b = 64'b0;
    end
    apply_check($sformatf("op %h pred %0d imm %0d rt %0d v %0d jm %0d", op, pred, use_imm, rt, v, junk_mode), e);
  endtask

  // ld and st: a register, a base register, then an index register or a displacement.
  task automatic t_mem(input bit is_st, input logic [1:0] size, input bit sext, input bit use_imm,
                       input int rr, input int rb, input int rx, input logic signed [63:0] v);
    exp_t e;
    begin_insn(is_st ? 8'h41 : 8'h40);
    put_reg(rr);
    put_reg(rb);
    if (use_imm) put_imm(v); else put_reg(rx);
    end_insn({sext, size, use_imm});
    e = '{default: '0};
    e.chk_alu = 1'b1; e.chk_a = 1'b1; e.chk_b = 1'b1; e.chk_mem = 1'b1;
    e.alu_op   = 4'h0;
    e.a        = rf[rb];
    e.b        = use_imm ? 64'(v) : rf[rx];
    e.mem_size = size;
    e.mem_sext = sext;
    e.is_load  = !is_st;
    e.is_store = is_st;
    if (is_st) begin
      e.chk_c = 1'b1;
      e.c     = rf[rr];
    end else begin
      e.chk_rd = 1'b1;
      e.rd     = 5'(rr);
      e.wr_en  = 1'b1;
    end
    apply_check($sformatf("%s size %0d sext %0d imm %0d r %0d base %0d idx %0d v %0d jm %0d",
                          is_st ? "st" : "ld", size, sext, use_imm, rr, rb, rx, v, junk_mode), e);
  endtask

  task automatic t_trap(input logic signed [63:0] v);
    exp_t e;
    begin_insn(8'hFE);
    put_imm(v);
    end_insn(4'h0);
    e = '{default: '0};
    e.chk_a = 1'b1; e.chk_b = 1'b1;
    e.is_trap = 1'b1;
    e.a       = 64'(v);
    apply_check($sformatf("trap v %0d jm %0d", v, junk_mode), e);
  endtask

  task automatic t_nop_halt(input logic [7:0] op);
    exp_t e;
    begin_insn(op);
    end_insn(4'h0);
    e = '{default: '0};
    e.is_halt = (op == 8'hFF);
    apply_check($sformatf("op %h jm %0d", op, junk_mode), e);
  endtask

  // The values sit on the border of each immediate width, so a wrong sign bit or a wrong
  // byte count shows at once. The last values have a random width.
  logic signed [63:0] vals [NV];

  task automatic init_values();
    logic [63:0] r;
    int          w;
    vals[0]  = 64'sd0;
    vals[1]  = 64'sd1;
    vals[2]  = -64'sd1;
    vals[3]  = 64'sd127;
    vals[4]  = 64'sd128;
    vals[5]  = -64'sd128;
    vals[6]  = -64'sd129;
    vals[7]  = 64'sd255;
    vals[8]  = 64'sd256;
    vals[9]  = 64'sd32767;
    vals[10] = 64'sd32768;
    vals[11] = -64'sd32768;
    vals[12] = -64'sd32769;
    vals[13] = 64'sd8388607;
    vals[14] = 64'sd8388608;
    vals[15] = -64'sd8388608;
    vals[16] = -64'sd8388609;
    vals[17] = 64'sh0000_0000_7FFF_FFFF;
    vals[18] = 64'sh0000_0000_8000_0000;
    vals[19] = 64'shFFFF_FFFF_8000_0000;
    vals[20] = 64'shFFFF_FFFF_7FFF_FFFF;
    vals[21] = 64'sh0000_007F_FFFF_FFFF;
    vals[22] = 64'sh0000_0080_0000_0000;
    vals[23] = 64'shFFFF_FF80_0000_0000;
    vals[24] = 64'shFFFF_FF7F_FFFF_FFFF;
    vals[25] = 64'sh0000_7FFF_FFFF_FFFF;
    vals[26] = 64'sh0000_8000_0000_0000;
    vals[27] = 64'shFFFF_8000_0000_0000;
    vals[28] = 64'shFFFF_7FFF_FFFF_FFFF;
    vals[29] = 64'sh007F_FFFF_FFFF_FFFF;
    vals[30] = 64'sh0080_0000_0000_0000;
    vals[31] = 64'shFF80_0000_0000_0000;
    vals[32] = 64'shFF7F_FFFF_FFFF_FFFF;
    vals[33] = 64'sh7FFF_FFFF_FFFF_FFFF;
    vals[34] = 64'sh8000_0000_0000_0000;
    for (int i = 35; i < NV; i++) begin
      r = {$urandom, $urandom};
      w = $urandom_range(1, 8);
      vals[i] = $signed(r << (64 - 8 * w)) >>> (64 - 8 * w);
    end
  endtask

  task automatic test_directed();
    for (int jm = 0; jm < 4; jm++) begin
      junk_mode = jm;
      pc_val    = {$urandom, 32'($urandom) & 32'hFFFF_FFFC};

      t_nop_halt(8'h00);
      t_nop_halt(8'hFF);

      // Register forms: each register number goes through each read port.
      for (int r = 0; r < 32; r++) begin
        t_unary(8'h01, r, (r * 5 + 3) % 32, 1'b0, 64'sd0);
        t_unary(8'h14, r, (r * 5 + 3) % 32, 1'b0, 64'sd0);
        t_cmp(8'h20, r, (r * 7 + 1) % 32, 1'b0, 64'sd0);
        t_cmp(8'h21, r, (r * 7 + 1) % 32, 1'b0, 64'sd0);
        for (int o = 16; o <= 26; o++)
          if (o != 20) t_alu(8'(o), r, (r + 11) % 32, (r * 3 + 5) % 32, 1'b0, 64'sd0);
        t_branch(8'h30, 3'(r % 7), 1'b0, r, 64'sd0);
        t_branch(8'h31, 3'(r % 8), 1'b0, r, 64'sd0);
        t_mem(1'b0, 2'(r % 4), r[3], 1'b0, r, (r + 9) % 32, (r * 3 + 2) % 32, 64'sd0);
        t_mem(1'b1, 2'(r % 4), 1'b0, 1'b0, r, (r + 9) % 32, (r * 3 + 2) % 32, 64'sd0);
      end

      // Immediate forms: each width in each format.
      for (int k = 0; k < NV; k++) begin
        t_unary(8'h01, 3, 7, 1'b1, vals[k]);
        t_unary(8'h14, 4, 8, 1'b1, vals[k]);
        t_cmp(8'h20, 5, 9, 1'b1, vals[k]);
        t_cmp(8'h21, 6, 10, 1'b1, vals[k]);
        for (int o = 16; o <= 26; o++)
          if (o != 20) t_alu(8'(o), 2, 4, 6, 1'b1, vals[k]);
        t_branch(8'h30, 3'(k % 7), 1'b1, 0, vals[k]);
        t_branch(8'h31, 3'(k % 8), 1'b1, 0, vals[k]);
        t_mem(1'b0, 2'(k % 4), k[2], 1'b1, 12, 13, 0, vals[k]);
        t_mem(1'b1, 2'(k % 4), 1'b0, 1'b1, 14, 15, 0, vals[k]);
        t_trap(vals[k]);
      end
    end
  endtask

  // ---- Opcode, flag and length sweep --------------------------------------------------

  // Every opcode, flag nibble and length gets a frame with valid register numbers. The
  // frame is legal exactly when the model says so, and a legal frame must give the
  // control outputs of its instruction.
  task automatic check_control(input logic [7:0] op, input logic [3:0] fl);
    string tag;
    tag = $sformatf("sweep op %h flags %h", op, fl);
    check64(64'(ex_is_branch), 64'(op == 8'h30 || op == 8'h31), {tag, " is_branch"});
    check64(64'(ex_is_load),   64'(op == 8'h40),                {tag, " is_load"});
    check64(64'(ex_is_store),  64'(op == 8'h41),                {tag, " is_store"});
    check64(64'(ex_is_trap),   64'(op == 8'hFE),                {tag, " is_trap"});
    check64(64'(ex_is_halt),   64'(op == 8'hFF),                {tag, " is_halt"});
    check64(64'(ex_wr_en),
            64'(op == 8'h01 || (op >= 8'h10 && op <= 8'h1A) || op == 8'h40), {tag, " wr_en"});
    if (op == 8'h30) check64(64'(ex_pred), 64'(fl[2:0]), {tag, " pred"});
    if (op == 8'h31) check64(64'(ex_pred), 64'b0, {tag, " pred"});
    if (op == 8'h40 || op == 8'h41) begin
      check64(64'(ex_mem_size), 64'(fl[2:1]), {tag, " mem_size"});
      check64(64'(ex_mem_sext), 64'(fl[3]),   {tag, " mem_sext"});
    end
    if (op >= 8'h10 && op <= 8'h1A) check64(64'(ex_alu_op), 64'(op[3:0]), {tag, " alu_op"});
    if (op == 8'h20 || op == 8'h21) check64(64'(ex_alu_op), op[0] ? 64'hC : 64'hB, {tag, " alu_op"});
    if (op == 8'h01 || op == 8'h30 || op == 8'h31 || op == 8'h40 || op == 8'h41)
      check64(64'(ex_alu_op), 64'h0, {tag, " alu_op"});
  endtask

  task automatic test_sweep();
    bit ill;
    junk_mode = 0;
    for (int op = 0; op < 256; op++)
      for (int fl = 0; fl < 16; fl++)
        for (int ln = 0; ln < 16; ln++) begin
          begin_insn(8'(op));
          fbv = with_byte(fbv, 2, 8'd1);
          fbv = with_byte(fbv, 3, 8'd2);
          fbv = with_byte(fbv, 4, 8'd3);
          pack_frame(ln, 4'(fl));
          frame_valid = 1'b1;
          #1;
          ill = ref_illegal(8'(op), 4'(fl), ln, 8'd1, 8'd2, 8'd3);
          check64(64'(ex_illegal), 64'(ill), $sformatf("sweep op %h flags %h len %0d illegal", op, fl, ln));
          if (!ill) check_control(8'(op), 4'(fl));
        end
  endtask

  // The register bytes come from the whole range, so each register byte is tested at
  // each position and for each format.
  task automatic test_random_illegal();
    logic [7:0] op, b2, b3, b4;
    logic [3:0] fl;
    int         ln;
    junk_mode = 0;
    for (int n = 0; n < 60000; n++) begin
      op = ($urandom_range(0, 1) == 0) ? legal_ops[$urandom_range(0, 20)] : 8'($urandom);
      fl = 4'($urandom);
      ln = $urandom_range(0, 15);
      b2 = ($urandom_range(0, 1) == 0) ? 8'($urandom_range(0, 31)) : 8'($urandom);
      b3 = ($urandom_range(0, 1) == 0) ? 8'($urandom_range(0, 31)) : 8'($urandom);
      b4 = ($urandom_range(0, 1) == 0) ? 8'($urandom_range(0, 31)) : 8'($urandom);
      begin_insn(op);
      fbv = with_byte(fbv, 2, b2);
      fbv = with_byte(fbv, 3, b3);
      fbv = with_byte(fbv, 4, b4);
      pack_frame(ln, fl);
      frame_valid = 1'b1;
      #1;
      check64(64'(ex_illegal), 64'(ref_illegal(op, fl, ln, b2, b3, b4)),
              $sformatf("random op %h flags %h len %0d regs %h %h %h illegal", op, fl, ln, b2, b3, b4));
    end
  endtask

  // ---- Frames that must fault ----------------------------------------------------------

  // Each frame that must fault has a look-alike with one change that must not fault. A
  // decoder that raised ex_illegal for every frame, or for none, would fail one of the
  // two kinds.
  int fault_total = 0;
  int fault_hit   = 0;
  int legal_total = 0;
  int legal_hit   = 0;

  task automatic expect_illegal(input string name, input bit want);
    frame_valid = 1'b1;
    #1;
    check64(64'(ex_illegal), 64'(want),
            {"fault test: ", name, want ? " must raise ex_illegal" : " must not raise ex_illegal"});
    check64(ex_pc, frame_pc, {"fault test: ", name, " pc"});
    if (want) begin
      fault_total++;
      if (ex_illegal === 1'b1) fault_hit++;
    end else begin
      legal_total++;
      if (ex_illegal === 1'b0) legal_hit++;
    end
  endtask

  // The task put_reg accepts only defined register numbers.
  task automatic put_raw(input logic [7:0] b);
    fbv = with_byte(fbv, pos, b);
    pos++;
  endtask

  // The register bytes are 1, 2 and 3, except the byte at bad_pos.
  task automatic reg_frame(input logic [7:0] op, input logic [3:0] flags, input int nregs,
                           input int bad_pos, input logic [7:0] val);
    begin_insn(op);
    for (int i = 0; i < nregs; i++) put_raw((pos == bad_pos) ? val : 8'(i + 1));
    end_insn(flags);
  endtask

  function automatic logic [7:0] bad_reg_value(input int k);
    case (k)
      0:       bad_reg_value = 8'd32;
      1:       bad_reg_value = 8'd33;
      2:       bad_reg_value = 8'h80;
      default: bad_reg_value = 8'hFF;
    endcase
  endfunction

  // Register 31 is the last defined register. Register 32 is the first that is not.
  task automatic reg_case(input string nm, input logic [7:0] op, input logic [3:0] flags,
                          input int nregs);
    for (int p = 2; p < 2 + nregs; p++) begin
      reg_frame(op, flags, nregs, p, 8'd31);
      expect_illegal($sformatf("%s with r31 in byte %0d", nm, p), 1'b0);
      for (int k = 0; k < 4; k++) begin
        reg_frame(op, flags, nregs, p, bad_reg_value(k));
        expect_illegal($sformatf("%s with register %0d in byte %0d", nm, bad_reg_value(k), p), 1'b1);
      end
    end
  endtask

  task automatic test_faults();
    logic [7:0] bad_op  [11];
    logic [7:0] good_op [11];

    junk_mode = 0;

    // Each opcode that is not defined is next to a defined opcode. The frames are the same.
    bad_op  = '{8'h02, 8'h0F, 8'h1B, 8'h1F, 8'h22, 8'h32, 8'h42, 8'h50, 8'h80, 8'hFD, 8'hFC};
    good_op = '{8'h01, 8'h00, 8'h1A, 8'h10, 8'h21, 8'h31, 8'h41, 8'h40, 8'h00, 8'hFE, 8'hFF};
    for (int i = 0; i < 11; i++) begin
      begin_insn(bad_op[i]);
      put_raw(8'd1);
      put_raw(8'd2);
      put_raw(8'd3);
      end_insn(4'h0);
      expect_illegal($sformatf("undefined opcode %h", bad_op[i]), 1'b1);
      begin_insn(good_op[i]);
      put_raw(8'd1);
      put_raw(8'd2);
      put_raw(8'd3);
      end_insn(4'h0);
      expect_illegal($sformatf("defined opcode %h", good_op[i]), 1'b0);
    end

    // Fetch flags a length below 2. A length above 12 is more than the longest instruction.
    for (int ln = 0; ln < 16; ln++)
      if (ln < 2 || ln == 5 || ln > 12) begin
        begin_insn(8'h10);
        put_raw(8'd1);
        put_raw(8'd2);
        put_raw(8'd3);
        pack_frame(ln, 4'h0);
        expect_illegal($sformatf("add r1, r2, r3 with length %0d", ln), ln != 5);
      end
    begin_insn(8'h10);
    put_raw(8'd1);
    put_raw(8'd2);
    repeat (8) put_raw(8'h11);
    end_insn(4'h1);
    expect_illegal("add with an 8-byte immediate (length 12)", 1'b0);

    reg_case("mov rd, rs",         8'h01, 4'h0, 2);
    reg_case("not rd, rs",         8'h14, 4'h0, 2);
    reg_case("cmp ra, rs",         8'h20, 4'h0, 2);
    reg_case("add rd, rs1, rs2",   8'h10, 4'h0, 3);
    reg_case("ld rd, [rb + rx]",   8'h40, 4'h0, 3);
    reg_case("st rs, [rb + rx]",   8'h41, 4'h0, 3);
    reg_case("b register target",  8'h30, 4'h0, 1);
    reg_case("jmp register target", 8'h31, 4'h0, 1);

    // The same byte values are legal where the byte is part of an immediate, or where the
    // instruction does not use the byte.
    begin_insn(8'h01);
    put_raw(8'd1);
    put_raw(8'hFF);
    end_insn(4'h1);
    expect_illegal("mov r1, #-1 (byte 3 is 0xFF)", 1'b0);
    begin_insn(8'h10);
    put_raw(8'd1);
    put_raw(8'd2);
    put_raw(8'h80);
    end_insn(4'h1);
    expect_illegal("add r1, r2, #-128 (byte 4 is 0x80)", 1'b0);
    begin_insn(8'h30);
    put_raw(8'hF8);
    end_insn(4'h8);
    expect_illegal("b #-8 (byte 2 is 0xF8)", 1'b0);
    begin_insn(8'hFE);
    put_raw(8'hFF);
    end_insn(4'h0);
    expect_illegal("trap #-1 (byte 2 is 0xFF)", 1'b0);
    junk_mode = 1;
    begin_insn(8'h00);
    end_insn(4'h0);
    expect_illegal("nop followed by 0xFF bytes", 1'b0);
    begin_insn(8'hFF);
    end_insn(4'h0);
    expect_illegal("halt followed by 0xFF bytes", 1'b0);
    junk_mode = 0;

    // The predicate 7 is not defined. The simulator does not test the predicate bits of a jmp.
    for (int p = 0; p < 8; p++) begin
      begin_insn(8'h30);
      put_raw(8'hF8);
      end_insn({1'b1, 3'(p)});
      expect_illegal($sformatf("b #-8 with predicate %0d", p), p == 7);
      begin_insn(8'h30);
      put_raw(8'd1);
      end_insn({1'b0, 3'(p)});
      expect_illegal($sformatf("b r1 with predicate %0d", p), p == 7);
    end
    begin_insn(8'h31);
    put_raw(8'd1);
    end_insn(4'h7);
    expect_illegal("jmp r1 with the predicate bits 7", 1'b0);

    // An immediate has at most 8 bytes. For b, jmp and trap the length limit of 12 does not
    // stop a longer immediate, because the immediate starts at byte 2.
    for (int n = 7; n <= 10; n++) begin
      begin_insn(8'h31);
      repeat (n) put_raw(8'h11);
      end_insn(4'h8);
      expect_illegal($sformatf("jmp with a %0d-byte immediate", n), n > 8);
      begin_insn(8'h30);
      repeat (n) put_raw(8'h11);
      end_insn(4'h8);
      expect_illegal($sformatf("b with a %0d-byte immediate", n), n > 8);
      begin_insn(8'hFE);
      repeat (n) put_raw(8'h11);
      end_insn(4'h0);
      expect_illegal($sformatf("trap with a %0d-byte immediate", n), n > 8);
    end
    for (int n = 7; n <= 9; n++) begin
      begin_insn(8'h01);
      put_raw(8'd1);
      repeat (n) put_raw(8'h11);
      end_insn(4'h1);
      expect_illegal($sformatf("mov with a %0d-byte immediate", n), n > 8);
      begin_insn(8'h10);
      put_raw(8'd1);
      put_raw(8'd2);
      repeat (n) put_raw(8'h11);
      end_insn(4'h1);
      expect_illegal($sformatf("add with a %0d-byte immediate", n), n > 8);
    end
  endtask

  // ---- Halt latch and handshake ------------------------------------------------------

  task automatic do_reset();
    rst_n          = 1'b0;
    redirect_valid = 1'b0;
    frame_valid    = 1'b0;
    ex_ready       = 1'b1;
    tick(3);
    rst_n = 1'b1;
  endtask

  task automatic test_halt();
    junk_mode = 0;
    begin_insn(8'hFF);
    end_insn(4'h0);
    do_reset();
    check64(64'(halt), 64'b0, "halt after reset");

    // Execute does not accept the frame, so the latch must not change.
    frame_valid = 1'b1;
    ex_ready    = 1'b0;
    tick(3);
    check64(64'(halt), 64'b0, "halt while Execute is not ready");
    ex_ready = 1'b1;
    tick();
    check64(64'(halt), 64'b1, "halt after Execute accepts");
    frame_valid = 1'b0;
    tick(3);
    check64(64'(halt), 64'b1, "halt stays");

    // A redirect removes a halt of the wrong path.
    redirect_valid = 1'b1;
    tick();
    redirect_valid = 1'b0;
    check64(64'(halt), 64'b0, "redirect clears halt");

    // A halt frame in the cycle of a redirect is on the wrong path.
    frame_valid    = 1'b1;
    redirect_valid = 1'b1;
    #1;
    check64(64'(ex_valid), 64'b0, "ex_valid low in a redirect");
    tick();
    check64(64'(halt), 64'b0, "halt frame in a redirect");
    redirect_valid = 1'b0;
    frame_valid    = 1'b0;
    tick();

    // An illegal halt has no side effect.
    begin_insn(8'hFF);
    pack_frame(1, 4'h0);
    frame_valid = 1'b1;
    tick(2);
    check64(64'(ex_illegal), 64'b1, "halt with length 1 is illegal");
    check64(64'(halt), 64'b0, "illegal halt does not set the latch");
    frame_valid = 1'b0;

    // A reset removes the latch.
    begin_insn(8'hFF);
    end_insn(4'h0);
    frame_valid = 1'b1;
    tick(2);
    check64(64'(halt), 64'b1, "halt before reset");
    rst_n = 1'b0;
    tick(2);
    check64(64'(halt), 64'b0, "reset clears halt");
    rst_n       = 1'b1;
    frame_valid = 1'b0;
    tick();
  endtask

  task automatic test_handshake();
    for (int v = 0; v < 2; v++)
      for (int r = 0; r < 2; r++)
        for (int d = 0; d < 2; d++) begin
          frame_valid    = 1'(v);
          ex_ready       = 1'(r);
          redirect_valid = 1'(d);
          #1;
          check64(64'(frame_ready), 64'(r), "frame_ready follows ex_ready");
          check64(64'(ex_valid), 64'(v == 1 && d == 0), "ex_valid");
        end
    frame_valid    = 1'b0;
    ex_ready       = 1'b1;
    redirect_valid = 1'b0;
  endtask

  initial begin
    for (int i = 0; i < 32; i++) rf[i] = {16'hC0DE, 16'(i), 16'hBEEF, 16'(i * 7 + 1)};
    legal_ops = '{8'h00, 8'h01, 8'h10, 8'h11, 8'h12, 8'h13, 8'h14, 8'h15, 8'h16, 8'h17,
                  8'h18, 8'h19, 8'h1A, 8'h20, 8'h21, 8'h30, 8'h31, 8'h40, 8'h41, 8'hFE, 8'hFF};
    init_values();
    do_reset();

    test_directed();
    test_faults();
    test_sweep();
    test_random_illegal();
    test_halt();
    test_handshake();

    $display("tb_decode: %0d of %0d frames that must fault raised ex_illegal, %0d of %0d look-alikes did not",
             fault_hit, fault_total, legal_hit, legal_total);
    $display("tb_decode: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_decode failed");
    $finish;
  end

  initial begin
    #50000000;
    $fatal(1, "tb_decode watchdog");
  end
endmodule
