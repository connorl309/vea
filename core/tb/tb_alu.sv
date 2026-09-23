//! Testbench for vea_alu. See ../alu.sv for the module. Every check is one clock edge
//! after driving new inputs, so the registers of ADD and SUB load. Only those two results
//! come from registers.

module tb_alu #(
  parameter int SEED = 0
);
  localparam logic [3:0] OP_ADD = 4'h0, OP_SUB = 4'h1, OP_AND = 4'h2, OP_OR = 4'h3;
  localparam logic [3:0] OP_NOT = 4'h4, OP_XOR = 4'h5, OP_SHL = 4'h6, OP_SHR = 4'h7;
  localparam logic [3:0] OP_SAR = 4'h8, OP_MUL = 4'h9, OP_DIV = 4'hA;
  localparam logic [3:0] OP_CMP = 4'hB, OP_CMP_S = 4'hC;

  logic clk;

  initial begin
    clk = 1'b0;
    forever #5 clk = ~clk;
  end

  logic [63:0] a = '0;
  logic [63:0] b = '0;
  logic [3:0]  op = '0;
  logic [63:0] result;
  logic [63:0] add_result;
  logic [3:0]  flags;
  logic        flags_valid;
  logic        unsupported;
  logic        late;

  vea_alu dut (.*);

  int checks = 0;
  int errors = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      // A systematic error would print thousands of lines.
      if (errors <= 40) $display("FAIL: tb_alu: %s: got %h, expected %h", what, got, want);
    end
  endtask

  // The result of ADD and SUB must come from the registers only. So after the edge, a and
  // b change to random values, and a design that reads them directly gets a wrong result.
  // Every other operation is combinational and keeps its operands.
  task automatic apply(input logic [63:0] av, input logic [63:0] bv, input logic [3:0] opv);
    a  = av;
    b  = bv;
    op = opv;
    @(posedge clk);
    #1;
    if (opv == OP_ADD || opv == OP_SUB) begin
      a = {$urandom, $urandom};
      b = {$urandom, $urandom};
      #1;
    end
  endtask

  // ---- Reference model ------------------------------------------------------------------

  // Bit order matches vea_alu's flags_t: zero, neg, carry, overflow, MSB first.
  function automatic logic [3:0] ref_flags(input bit zero, input bit neg, input bit carry,
                                            input bit overflow);
    ref_flags = {zero, neg, carry, overflow};
  endfunction

  task automatic ref_model(input logic [63:0] av, input logic [63:0] bv, input logic [3:0] opv,
                            output logic [63:0] res, output logic [3:0] fl,
                            output logic fl_valid, output logic unsupp,
                            output logic is_late);
    logic [63:0] diff;
    logic [5:0]  shamt;
    bit          sub_ovf;

    diff    = av - bv;
    shamt   = bv[5:0];
    sub_ovf = (av[63] != bv[63]) && (diff[63] != av[63]);

    res      = '0;
    fl       = '0;
    fl_valid = 1'b0;
    unsupp   = 1'b0;
    is_late  = (opv == OP_ADD) || (opv == OP_SUB);

    case (opv)
      OP_ADD: res = av + bv;
      OP_SUB: res = diff;
      OP_AND: res = av & bv;
      OP_OR:  res = av | bv;
      OP_NOT: res = ~av;
      OP_XOR: res = av ^ bv;
      OP_SHL: res = av << shamt;
      OP_SHR: res = av >> shamt;
      OP_SAR: res = $signed(av) >>> shamt;

      // vea_mul computes MUL, not this ALU. res and unsupp stay at their default.
      OP_MUL: ;

      OP_CMP: begin
        fl_valid = 1'b1;
        fl       = ref_flags(av == bv, av < bv, av >= bv, 1'b0);
      end

      OP_CMP_S: begin
        fl_valid = 1'b1;
        fl       = ref_flags(av == bv, diff[63], av >= bv, sub_ovf);
      end

      OP_DIV: unsupp = 1'b1;
      default: unsupp = 1'b1;
    endcase
  endtask

  task automatic check_op(input string name, input logic [63:0] av, input logic [63:0] bv,
                           input logic [3:0] opv);
    logic [63:0] want_res;
    logic [3:0]  want_flags;
    logic        want_flags_valid, want_unsupp, want_late;

    ref_model(av, bv, opv, want_res, want_flags, want_flags_valid, want_unsupp, want_late);
    apply(av, bv, opv);

    check64(result,             want_res,             {name, ": result"});
    // add_result is the ADD register, so it holds a + b for every op.
    check64(add_result,         av + bv,               {name, ": add_result"});
    check64(64'(flags),         64'(want_flags),       {name, ": flags"});
    check64(64'(flags_valid),   64'(want_flags_valid), {name, ": flags_valid"});
    check64(64'(unsupported),   64'(want_unsupp),      {name, ": unsupported"});
    check64(64'(late),          64'(want_late),        {name, ": late"});
  endtask

  // ---- Directed operand pairs, reused across ops -----------------------------------------

  localparam int N_DIRECTED = 8;
  logic [63:0] dv [N_DIRECTED];

  function automatic void init_directed();
    dv[0] = 64'h0;
    dv[1] = 64'h1;
    dv[2] = 64'hFFFF_FFFF_FFFF_FFFF;
    dv[3] = 64'h8000_0000_0000_0000; // INT64_MIN
    dv[4] = 64'h7FFF_FFFF_FFFF_FFFF; // INT64_MAX
    dv[5] = 64'h5555_5555_5555_5555;
    dv[6] = 64'hAAAA_AAAA_AAAA_AAAA;
    dv[7] = 64'hDEAD_BEEF_CAFE_F00D;
  endfunction

  // Every directed pair, then N random pairs, all through the same op.
  task automatic sweep_op(input string name, input logic [3:0] opv, input int n_random);
    for (int i = 0; i < N_DIRECTED; i++) begin
      for (int j = 0; j < N_DIRECTED; j++) begin
        check_op($sformatf("%s directed a=%0d b=%0d", name, i, j), dv[i], dv[j], opv);
      end
    end
    for (int n = 0; n < n_random; n++) begin
      check_op($sformatf("%s random %0d", name, n), {$urandom, $urandom}, {$urandom, $urandom},
                opv);
    end
  endtask

  // ---- Tests --------------------------------------------------------------------------

  // ADD, SUB, AND, OR, XOR: plain 64-bit results, no flags.
  task automatic test_arith_logic();
    sweep_op("add", OP_ADD, 2000);
    sweep_op("sub", OP_SUB, 2000);
    sweep_op("and", OP_AND, 2000);
    sweep_op("or",  OP_OR,  2000);
    sweep_op("xor", OP_XOR, 2000);
  endtask

  // NOT reads only a. Sweep b across every directed value while a is fixed, so a b that
  // leaks into the result would show up as a wrong result for the same a.
  task automatic test_not();
    for (int i = 0; i < N_DIRECTED; i++) begin
      for (int j = 0; j < N_DIRECTED; j++) begin
        check_op($sformatf("not a=%0d b=%0d", i, j), dv[i], dv[j], OP_NOT);
      end
    end
    for (int n = 0; n < 500; n++) begin
      check_op($sformatf("not random %0d", n), {$urandom, $urandom}, {$urandom, $urandom},
                OP_NOT);
    end
  endtask

  // The simulator uses only the low 6 bits of b as the shift amount, so a shift amount at
  // and past 64 must wrap instead of clearing the result the way a native 64-bit shift by
  // an out-of-range amount would.
  task automatic test_shifts();
    logic [63:0] shamts [12];

    shamts = '{64'd0, 64'd1, 64'd4, 64'd31, 64'd32, 64'd63, 64'd64, 64'd65, 64'd95, 64'd127,
               64'd128, 64'hFFFF_FFFF_FFFF_FFC0};

    foreach (dv[i]) begin
      for (int j = 0; j < 12; j++) begin
        check_op($sformatf("shl a=%0d shamt=%0d", i, j), dv[i], shamts[j], OP_SHL);
        check_op($sformatf("shr a=%0d shamt=%0d", i, j), dv[i], shamts[j], OP_SHR);
        check_op($sformatf("sar a=%0d shamt=%0d", i, j), dv[i], shamts[j], OP_SAR);
      end
    end
    for (int n = 0; n < 500; n++) begin
      check_op($sformatf("shl random %0d", n), {$urandom, $urandom}, {$urandom, $urandom},
                OP_SHL);
      check_op($sformatf("shr random %0d", n), {$urandom, $urandom}, {$urandom, $urandom},
                OP_SHR);
      check_op($sformatf("sar random %0d", n), {$urandom, $urandom}, {$urandom, $urandom},
                OP_SAR);
    end
  endtask

  // Unsigned compare: directed pairs cover equal, less, greater and the extremes, plus the
  // reference model's own zero/neg/carry math against random pairs.
  task automatic test_cmp();
    sweep_op("cmp", OP_CMP, 2000);
  endtask

  // Signed compare: same coverage, plus directed overflow-on-subtract cases (INT64_MIN vs
  // a positive value and back), since sub_overflow only fires on a narrow input range.
  task automatic test_cmp_s();
    sweep_op("cmp.s", OP_CMP_S, 2000);
    check_op("cmp.s overflow, min - 1",  64'h8000_0000_0000_0000, 64'h1, OP_CMP_S);
    check_op("cmp.s overflow, max - -1", 64'h7FFF_FFFF_FFFF_FFFF, -64'sd1, OP_CMP_S);
    check_op("cmp.s no overflow, equal", 64'h8000_0000_0000_0000, 64'h8000_0000_0000_0000,
              OP_CMP_S);
  endtask

  // MUL is legal, but this ALU takes no part in it. vea_mul computes the result instead
  // (see tb_mul.sv). Here MUL must just leave result, flags and unsupported at zero, for
  // every operand pair.
  task automatic test_mul();
    sweep_op("mul", OP_MUL, 2000);
  endtask

  // DIV, and every undefined opcode (0xD-0xF), must raise unsupported and leave the result
  // and flags at their default zero, regardless of the operands.
  task automatic test_unsupported();
    logic [3:0] undefined_ops [3];
    undefined_ops = '{4'hD, 4'hE, 4'hF};

    for (int i = 0; i < N_DIRECTED; i++) begin
      for (int j = 0; j < N_DIRECTED; j++) begin
        check_op($sformatf("div a=%0d b=%0d", i, j), dv[i], dv[j], OP_DIV);
      end
    end
    for (int n = 0; n < 200; n++) begin
      check_op($sformatf("div random %0d", n), {$urandom, $urandom}, {$urandom, $urandom},
                OP_DIV);
    end

    foreach (undefined_ops[k]) begin
      for (int n = 0; n < 200; n++) begin
        check_op($sformatf("undefined op %h random %0d", undefined_ops[k], n),
                  {$urandom, $urandom}, {$urandom, $urandom}, undefined_ops[k]);
      end
    end
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_alu: seed %0d", SEED);
    init_directed();
    $display("tb_alu: test_arith_logic");
    test_arith_logic();
    $display("tb_alu: test_not");
    test_not();
    $display("tb_alu: test_shifts");
    test_shifts();
    $display("tb_alu: test_cmp");
    test_cmp();
    $display("tb_alu: test_cmp_s");
    test_cmp_s();
    $display("tb_alu: test_mul");
    test_mul();
    $display("tb_alu: test_unsupported");
    test_unsupported();

    $display("tb_alu: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_alu failed");
    $finish;
  end

  initial begin
    #50000000;
    $fatal(1, "tb_alu watchdog");
  end
endmodule
