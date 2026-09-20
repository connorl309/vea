//! Testbench for vea_decode.
//!
//! The instruction bytes come from the ENCODE table of the assembler. Each
//! test sends one instruction and reads the latch of decode in the next cycle.
//! The register file is a model with a different value in each register. This
//! way a read from a wrong register shows as a wrong operand.

// A testbench compares values of different widths on purpose.
/* verilator lint_off WIDTHEXPAND */
/* verilator lint_off WIDTHTRUNC */

module tb_decode;
  import tb_util_pkg::*;

  localparam logic [3:0] CLASS_NOP     = `DEC_CLASS_NOP;
  localparam logic [3:0] CLASS_HALT    = `DEC_CLASS_HALT;
  localparam logic [3:0] CLASS_MOV     = `DEC_CLASS_MOV;
  localparam logic [3:0] CLASS_ALU     = `DEC_CLASS_ALU;
  localparam logic [3:0] CLASS_CMP     = `DEC_CLASS_CMP;
  localparam logic [3:0] CLASS_BRANCH  = `DEC_CLASS_BRANCH;
  localparam logic [3:0] CLASS_LOAD    = `DEC_CLASS_LOAD;
  localparam logic [3:0] CLASS_STORE   = `DEC_CLASS_STORE;
  localparam logic [3:0] CLASS_TRAP    = `DEC_CLASS_TRAP;
  localparam logic [3:0] CLASS_ILLEGAL = `DEC_CLASS_ILLEGAL;

  logic clk = 1'b0;
  logic rst = 1'b1;
  logic flush = 1'b0;
  logic insn_valid = 1'b0;
  logic [63:0] insn_pc = '0;
  logic [95:0] insn_bytes = '0;
  logic [3:0] insn_len = '0;
  logic ex_ready = 1'b1;
  wire insn_ready;
  wire [4:0] rf_raddr_a;
  wire [4:0] rf_raddr_b;
  wire [4:0] rf_raddr_c;
  wire [63:0] rf_rdata_a;
  wire [63:0] rf_rdata_b;
  wire [63:0] rf_rdata_c;
  wire ex_valid;
  wire [63:0] ex_pc;
  wire [3:0] ex_class;
  wire [7:0] ex_alu_op;
  wire [4:0] ex_rd;
  wire [63:0] ex_a;
  wire [63:0] ex_b;
  wire [63:0] ex_addr;
  wire [63:0] ex_store_data;
  wire [2:0] ex_pred;
  wire [1:0] ex_mem_size;
  wire ex_mem_sext;
  wire ex_cmp_signed;

  vea_decode dut (.*);

  always #5 clk <= ~clk;

  logic [63:0] regs [32];

  initial
  begin
    for (int i = 0; i < 32; i++)
      regs[i] = {32'hC0DE_0000, 27'h0, 5'(i)} + 64'h1_0000_0000 * i;
  end

  assign rf_rdata_a = regs[rf_raddr_a];
  assign rf_rdata_b = regs[rf_raddr_b];
  assign rf_rdata_c = regs[rf_raddr_c];

  //! The test drives inputs and reads outputs 1 time unit after the edge. This way it never races with the clock.
  task automatic tick();
    @(posedge clk);
    #1;
  endtask

  //! The bytes after the instruction get the value AA. Decode must ignore them, as the fetch documentation says.
  function automatic logic [95:0] frame(input logic [95:0] lit, input int n);
    logic [95:0] top_mask;
    top_mask = ~96'h0 << (96 - 8 * n);
    return (lit << (96 - 8 * n)) | (96'hAAAA_AAAA_AAAA_AAAA_AAAA_AAAA & ~top_mask);
  endfunction

  //! The length comes from the length nibble in byte 1, as in fetch.
  task automatic send(input logic [63:0] pc, input logic [95:0] bytes);
    insn_pc = pc;
    insn_bytes = bytes;
    insn_len = bytes[87:84];
    insn_valid = 1'b1;
    tick();
    insn_valid = 1'b0;
  endtask

  task automatic expect_class(input string name, input logic [3:0] cls);
    check_eq({name, ": valid"}, ex_valid, 1'b1);
    check_eq({name, ": class"}, ex_class, cls);
    check_eq({name, ": pc"}, ex_pc, insn_pc);
  endtask

  task automatic expect_mov(input string name, input logic [4:0] rd, input logic [63:0] a);
    expect_class(name, CLASS_MOV);
    check_eq({name, ": rd"}, ex_rd, rd);
    check_eq({name, ": a"}, ex_a, a);
  endtask

  task automatic expect_alu(input string name, input logic [7:0] op, input logic [4:0] rd,
                            input logic [63:0] a, input logic [63:0] b);
    expect_class(name, CLASS_ALU);
    check_eq({name, ": alu op"}, ex_alu_op, op);
    check_eq({name, ": rd"}, ex_rd, rd);
    check_eq({name, ": a"}, ex_a, a);
    check_eq({name, ": b"}, ex_b, b);
  endtask

  task automatic expect_cmp(input string name, input logic [63:0] a, input logic [63:0] b, input logic is_signed);
    expect_class(name, CLASS_CMP);
    check_eq({name, ": a"}, ex_a, a);
    check_eq({name, ": b"}, ex_b, b);
    check_eq({name, ": signed"}, ex_cmp_signed, is_signed);
  endtask

  task automatic expect_branch(input string name, input logic [63:0] addr, input logic [2:0] pred);
    expect_class(name, CLASS_BRANCH);
    check_eq({name, ": target"}, ex_addr, addr);
    check_eq({name, ": predicate"}, ex_pred, pred);
  endtask

  task automatic expect_load(input string name, input logic [4:0] rd, input logic [63:0] addr,
                             input logic [1:0] size, input logic sext);
    expect_class(name, CLASS_LOAD);
    check_eq({name, ": rd"}, ex_rd, rd);
    check_eq({name, ": address"}, ex_addr, addr);
    check_eq({name, ": size"}, ex_mem_size, size);
    check_eq({name, ": sign extend"}, ex_mem_sext, sext);
  endtask

  task automatic expect_store(input string name, input logic [63:0] addr, input logic [1:0] size,
                              input logic [63:0] data);
    expect_class(name, CLASS_STORE);
    check_eq({name, ": address"}, ex_addr, addr);
    check_eq({name, ": size"}, ex_mem_size, size);
    check_eq({name, ": data"}, ex_store_data, data);
  endtask

  task automatic test_control();
    send(64'h0, frame(16'h0020, 2));
    expect_class("nop", CLASS_NOP);
    send(64'h4, frame(16'hFF20, 2));
    expect_class("halt", CLASS_HALT);
    send(64'h8, frame(16'hFE20, 2));
    expect_class("trap without a vector", CLASS_TRAP);
    check_eq("trap without a vector: vector", ex_b, 64'h0);
    send(64'hC, frame(24'hFE300D, 3));
    expect_class("trap 0x0d", CLASS_TRAP);
    check_eq("trap 0x0d: vector", ex_b, 64'h0D);
  endtask

  task automatic test_mov_and_not();
    send(64'h0, frame(32'h01400102, 4));
    expect_mov("mov r1, r2", 5'd1, regs[2]);
    send(64'h0, frame(40'h0151011234, 5));
    expect_mov("mov r1, 0x1234", 5'd1, 64'h1234);
    send(64'h0, frame(32'h014105FF, 4));
    expect_mov("mov r5, -1 (sign extended)", 5'd5, 64'hFFFF_FFFF_FFFF_FFFF);
    send(64'h0, frame(88'h01B1031122334455667788, 11));
    expect_mov("mov r3, 8 byte immediate", 5'd3, 64'h1122_3344_5566_7788);
    send(64'h0, frame(88'h01B103FFEEDDCCBBAA9988, 11));
    expect_mov("mov r3, negative 8 byte immediate", 5'd3, 64'hFFEE_DDCC_BBAA_9988);
    send(64'h0, frame(32'h14400102, 4));
    expect_alu("not r1, r2", 8'h4, 5'd1, regs[2], 64'h0);
    send(64'h0, frame(32'h14410105, 4));
    expect_alu("not r1, 5", 8'h4, 5'd1, 64'h5, 64'h0);
  endtask

  //! Every ALU opcode has the same layout. The operation is the low nibble of the opcode.
  task automatic test_alu();
    for (int op = 8'h10; op <= 8'h1A; op++)
    begin
      if (op != 8'h14)
      begin
        send(64'h0, frame({8'(op), 32'h50010203}, 5));
        expect_alu($sformatf("alu %02h, registers", op), 8'(op & 8'h0F), 5'd1, regs[2], regs[3]);
      end
    end
    send(64'h0, frame(40'h1051010205, 5));
    expect_alu("add r1, r2, 5", 8'h0, 5'd1, regs[2], 64'h5);
    send(64'h0, frame(40'h11510101FF, 5));
    expect_alu("sub r1, r1, -1 (sign extended)", 8'h1, 5'd1, regs[1], 64'hFFFF_FFFF_FFFF_FFFF);
    send(64'h0, frame(48'h1261_0102_00FF, 6));
    expect_alu("and r1, r2, 0xff (not sign extended)", 8'h2, 5'd1, regs[2], 64'hFF);
    send(64'h0, frame(48'h106101021234, 6));
    expect_alu("add r1, r2, 0x1234", 8'h0, 5'd1, regs[2], 64'h1234);
    send(64'h0, frame(96'h10C1010211223344_55667788, 12));
    expect_alu("add r1, r2, 8 byte immediate", 8'h0, 5'd1, regs[2], 64'h1122_3344_5566_7788);
    //! An immediate of 0 bytes must give 0. The bytes after the instruction are not 0.
    send(64'h0, frame(32'h10410102, 4));
    expect_alu("add r1, r2, 0 (no immediate bytes)", 8'h0, 5'd1, regs[2], 64'h0);
  endtask

  task automatic test_cmp();
    send(64'h0, frame(32'h20400102, 4));
    expect_cmp("cmp r1, r2", regs[1], regs[2], 1'b0);
    send(64'h0, frame(32'h21400102, 4));
    expect_cmp("cmp.s r1, r2", regs[1], regs[2], 1'b1);
    send(64'h0, frame(32'h214101FD, 4));
    expect_cmp("cmp.s r1, -3", regs[1], 64'hFFFF_FFFF_FFFF_FFFD, 1'b1);
    send(64'h0, frame(24'h203101, 3));
    expect_cmp("cmp r1, 0 (no immediate bytes)", regs[1], 64'h0, 1'b0);
  endtask

  //! Decode gives execute the target and the predicate. Execute decides if the branch is taken.
  task automatic test_branch_and_jump();
    for (int p = 0; p <= 6; p++)
    begin
      send(64'h100, frame({8'h30, 4'h3, 4'(8 | p), 8'h08}, 3));
      expect_branch($sformatf("branch predicate %0d", p), 64'h108, 3'(p));
    end
    send(64'h100, frame(24'h3038F8, 3));
    expect_branch("b -8 (offset from the branch itself)", 64'hF8, 3'd0);
    send(64'h100, frame(40'h3058010000, 5));
    expect_branch("b 3 byte offset", 64'h100 + 64'h10000, 3'd0);
    send(64'h100, frame(24'h30300A, 3));
    expect_branch("b r10 (a register is absolute)", regs[10], 3'd0);
    send(64'h100, frame(24'h303101, 3));
    expect_branch("beq r1", regs[1], 3'd1);
    send(64'h100, frame(24'h313007, 3));
    expect_branch("jmp r7", regs[7], 3'd0);
    send(64'h100, frame(32'h31481000, 4));
    expect_branch("jmp 0x1000 (absolute)", 64'h1000, 3'd0);
    send(64'h100, frame(24'h3138FC, 3));
    expect_branch("jmp -4 (absolute, sign extended)", 64'hFFFF_FFFF_FFFF_FFFC, 3'd0);
  endtask

  task automatic test_load();
    send(64'h0, frame(32'h40410506, 4));
    expect_load("ld r5, [r6]", 5'd5, regs[6], 2'd0, 1'b0);
    send(64'h0, frame(40'h4051050610, 5));
    expect_load("ld r5, [r6 + 0x10]", 5'd5, regs[6] + 64'h10, 2'd0, 1'b0);
    send(64'h0, frame(40'h4050050607, 5));
    expect_load("ld r5, [r6 + r7]", 5'd5, regs[6] + regs[7], 2'd0, 1'b0);
    send(64'h0, frame(40'h4057010210, 5));
    expect_load("ld.w r1, [r2 + 0x10]", 5'd1, regs[2] + 64'h10, 2'd3, 1'b0);
    //! The flag nibble is: sign extend, size (2 bits), immediate. This table lists the size and sign of each load.
    for (int i = 0; i < 7; i++)
    begin
      logic [3:0] flags;
      logic [1:0] size;
      logic sext;
      case (i)
        0: begin flags = 4'h1; size = 2'd0; sext = 1'b0; end
        1: begin flags = 4'h3; size = 2'd1; sext = 1'b0; end
        2: begin flags = 4'h5; size = 2'd2; sext = 1'b0; end
        3: begin flags = 4'h7; size = 2'd3; sext = 1'b0; end
        4: begin flags = 4'hB; size = 2'd1; sext = 1'b1; end
        5: begin flags = 4'hD; size = 2'd2; sext = 1'b1; end
        default: begin flags = 4'hF; size = 2'd3; sext = 1'b1; end
      endcase
      send(64'h0, frame({8'h40, 4'h4, flags, 16'h0506}, 4));
      expect_load($sformatf("load flags %h", flags), 5'd5, regs[6], size, sext);
    end
  endtask

  task automatic test_store();
    send(64'h0, frame(32'h41410506, 4));
    expect_store("st [r6], r5", regs[6], 2'd0, regs[5]);
    send(64'h0, frame(32'h41430506, 4));
    expect_store("st.b [r6], r5", regs[6], 2'd1, regs[5]);
    send(64'h0, frame(32'h41450506, 4));
    expect_store("st.h [r6], r5", regs[6], 2'd2, regs[5]);
    send(64'h0, frame(32'h41470506, 4));
    expect_store("st.w [r6], r5", regs[6], 2'd3, regs[5]);
    send(64'h0, frame(40'h41510506FC, 5));
    expect_store("st [r6 - 4], r5", regs[6] - 64'h4, 2'd0, regs[5]);
    send(64'h0, frame(40'h4150050607, 5));
    expect_store("st [r6 + r7], r5", regs[6] + regs[7], 2'd0, regs[5]);
  endtask

  //! A fault must not stop decode. Decode marks the instruction, and the pc shows where it is.
  task automatic test_illegal();
    send(64'h80, frame(16'h9920, 2));
    expect_class("unknown opcode 0x99", CLASS_ILLEGAL);
    send(64'h80, frame(16'h0220, 2));
    expect_class("unknown opcode 0x02", CLASS_ILLEGAL);
    send(64'h80, frame(16'h0000, 2));
    expect_class("length 0", CLASS_ILLEGAL);
    send(64'h80, frame(16'h0010, 2));
    expect_class("length 1", CLASS_ILLEGAL);
    send(64'h80, frame(16'h00D0, 2));
    expect_class("length 13", CLASS_ILLEGAL);
    send(64'h80, frame(16'h00F0, 2));
    expect_class("length 15", CLASS_ILLEGAL);
    send(64'h80, frame(40'h1050200101, 5));
    expect_class("add, rd is r32", CLASS_ILLEGAL);
    send(64'h80, frame(40'h1050012001, 5));
    expect_class("add, rs1 is r32", CLASS_ILLEGAL);
    send(64'h80, frame(40'h10500102FF, 5));
    expect_class("add, rs2 is r255", CLASS_ILLEGAL);
    send(64'h80, frame(32'h01400120, 4));
    expect_class("mov, source is r32", CLASS_ILLEGAL);
    send(64'h80, frame(32'h20400140, 4));
    expect_class("cmp, second register is r64", CLASS_ILLEGAL);
    send(64'h80, frame(32'h40410520, 4));
    expect_class("ld, base is r32", CLASS_ILLEGAL);
    send(64'h80, frame(32'h41412006, 4));
    expect_class("st, data register is r32", CLASS_ILLEGAL);
    send(64'h80, frame(24'h303020, 3));
    expect_class("b, register target is r32", CLASS_ILLEGAL);
    send(64'h80, frame(24'h303F08, 3));
    expect_class("b, predicate 7", CLASS_ILLEGAL);
    send(64'h80, frame(96'h01C1031122334455667788_99, 12));
    expect_class("mov, 9 byte immediate", CLASS_ILLEGAL);
    //! The same byte value is legal as an immediate. The register check is only for register bytes.
    send(64'h80, frame(40'h10510102FF, 5));
    expect_class("add, immediate byte FF", CLASS_ALU);
    send(64'h80, frame(24'h303820, 3));
    expect_class("b, immediate byte 0x20", CLASS_BRANCH);
  endtask

  task automatic test_reset_state();
    rst = 1'b1;
    tick();
    rst = 1'b0;
    check_eq("after reset: latch empty", ex_valid, 1'b0);
    check_eq("after reset: ready", insn_ready, 1'b1);
    tick();
    check_eq("no instruction: latch stays empty", ex_valid, 1'b0);
  endtask

  task automatic test_latch_empties();
    send(64'h0, frame(16'h0020, 2));
    check_eq("latch full", ex_valid, 1'b1);
    tick();
    check_eq("latch empties when execute takes it", ex_valid, 1'b0);
  endtask

  //! The latch must hold its instruction while execute is not ready, and decode must say it is not ready.
  task automatic test_stall();
    ex_ready = 1'b0;
    send(64'h10, frame(16'h0020, 2));
    check_eq("stall: first instruction latched into an empty latch", ex_valid, 1'b1);
    check_eq("stall: not ready", insn_ready, 1'b0);
    insn_pc = 64'h20;
    insn_bytes = frame(16'hFF20, 2);
    insn_len = 4'd2;
    insn_valid = 1'b1;
    repeat (3) tick();
    check_eq("stall: latch keeps the old pc", ex_pc, 64'h10);
    check_eq("stall: latch keeps the old class", ex_class, CLASS_NOP);
    ex_ready = 1'b1;
    #1;
    check_eq("stall: ready when execute takes the old instruction", insn_ready, 1'b1);
    tick();
    insn_valid = 1'b0;
    check_eq("stall: new instruction latched", ex_valid, 1'b1);
    check_eq("stall: new pc", ex_pc, 64'h20);
    check_eq("stall: new class", ex_class, CLASS_HALT);
  endtask

  //! A new instruction can replace the old one in the same cycle. The pipeline has no bubble.
  task automatic test_back_to_back();
    send(64'h30, frame(16'h0020, 2));
    send(64'h34, frame(16'hFF20, 2));
    check_eq("back to back: latch stays full", ex_valid, 1'b1);
    check_eq("back to back: new pc", ex_pc, 64'h34);
    check_eq("back to back: new class", ex_class, CLASS_HALT);
    tick();
  endtask

  //! After a redirect, an instruction in the latch is on the wrong path. A new instruction in the same cycle is also wrong.
  task automatic test_flush();
    send(64'h40, frame(16'h0020, 2));
    check_eq("before flush: latch full", ex_valid, 1'b1);
    flush = 1'b1;
    tick();
    flush = 1'b0;
    check_eq("flush empties the latch", ex_valid, 1'b0);

    flush = 1'b1;
    insn_pc = 64'h50;
    insn_bytes = frame(16'h0020, 2);
    insn_len = 4'd2;
    insn_valid = 1'b1;
    tick();
    flush = 1'b0;
    insn_valid = 1'b0;
    check_eq("flush drops the instruction that arrives with it", ex_valid, 1'b0);
  endtask

  initial
  begin
    test_reset_state();
    test_control();
    test_mov_and_not();
    test_alu();
    test_cmp();
    test_branch_and_jump();
    test_load();
    test_store();
    test_illegal();
    test_latch_empties();
    test_stall();
    test_back_to_back();
    test_flush();
    report("tb_decode");
    $finish;
  end

  initial
  begin
    #1000000;
    $fatal(1, "tb_decode: timeout");
  end

endmodule
