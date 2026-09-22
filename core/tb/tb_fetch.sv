//! Testbench for vea_fetch.
//!
//! Each build uses one memory latency and one ready pattern. The Makefile runs the
//! matrix.

module tb_fetch #(
  parameter int LATENCY      = 1,
  parameter bit READY_TOGGLE = 1'b0
);
  localparam int MB = 13;
  localparam int ST_REQUEST = 0, ST_WAIT = 1, ST_DELIVER = 2, ST_DRAIN = 3;

  logic clk = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  logic                  imem_req_valid;
  logic                  imem_req_ready;
  logic [63:0]           imem_addr;
  logic                  imem_rvalid = 1'b0;
  logic [8*MB-1:0]       imem_rdata  = '0;
  logic                  redirect_valid = 1'b0;
  logic [63:2]           redirect_pc = '0;
  logic                  halt = 1'b0;
  logic                  frame_valid;
  logic                  frame_ready = 1'b0;
  logic [63:0]           frame_pc;
  logic [8*MB-1:0]       frame_bytes;
  logic                  frame_bad_len;

  vea_fetch #(.MAX_INSN_BYTES(MB)) dut (.*);

  logic [7:0] cyc = 8'd0;
  always @(posedge clk) cyc <= cyc + 8'd1;
  assign imem_req_ready = READY_TOGGLE ? cyc[0] : 1'b1;

  // The memory replies to every request it accepts, also to a request that a redirect
  // cancelled. Fetch must discard those replies.
  logic [7:0]  mem [1024];
  logic        waiting = 1'b0;
  int          cnt = 0;
  logic [63:0] addr_q;

  function automatic logic [8*MB-1:0] read_frame(input logic [63:0] a);
    logic [8*MB-1:0] d;
    for (int i = 0; i < MB; i++) d[8*MB-1-8*i -: 8] = mem[int'(a) + i];
    return d;
  endfunction

  always @(posedge clk) begin
    imem_rvalid <= 1'b0;
    if (!rst_n) begin
      waiting <= 1'b0;
    end else begin
      if (waiting) begin
        if (cnt == 1) begin
          imem_rvalid <= 1'b1;
          imem_rdata  <= read_frame(addr_q);
          waiting     <= 1'b0;
        end else begin
          cnt <= cnt - 1;
        end
      end
      if (imem_req_valid && imem_req_ready) begin
        if (LATENCY == 1) begin
          imem_rvalid <= 1'b1;
          imem_rdata  <= read_frame(imem_addr);
        end else begin
          waiting <= 1'b1;
          cnt     <= LATENCY - 1;
          addr_q  <= imem_addr;
        end
      end
    end
  end

  int errors = 0;

  task automatic check(input bit cond, input string msg);
    if (!cond) begin
      errors++;
      $display("FAIL: tb_fetch latency %0d, ready toggle %0d: %s (t=%0t)",
               LATENCY, READY_TOGGLE, msg, $time);
    end
  endtask

  // The tests read the outputs 1 ns after the clock edge, when they are stable.
  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  task automatic do_reset(input bit hold_halt = 1'b0);
    rst_n = 1'b0;
    redirect_valid = 1'b0;
    frame_ready = 1'b0;
    halt = hold_halt;
    tick(3);
    rst_n = 1'b1;
  endtask

  task automatic wait_frame();
    int guard = 0;
    while (!frame_valid && guard < 200) begin
      tick();
      guard++;
    end
    check(frame_valid, "frame never arrived");
  endtask

  task automatic ack_frame();
    frame_ready = 1'b1;
    tick();
    frame_ready = 1'b0;
  endtask

  task automatic expect_frame(input logic [63:0] pc);
    wait_frame();
    check(frame_pc === pc, $sformatf("frame_pc %h, expected %h", frame_pc, pc));
    check(frame_bytes === read_frame(pc), $sformatf("frame_bytes wrong for pc %h", pc));
    ack_frame();
  endtask

  // The argument rv is 1 when a reply must arrive in this cycle, 0 when it must not,
  // and -1 when it does not matter.
  task automatic wait_state(input int st, input int rv);
    int guard = 0;
    while (!((int'(dut.state) == st) && (rv < 0 || int'(imem_rvalid) == rv)) && guard < 200) begin
      tick();
      guard++;
    end
    check(guard < 200, $sformatf("state %0d with reply %0d never reached", st, rv));
  endtask

  task automatic fire_redirect(input logic [63:0] target);
    redirect_valid = 1'b1;
    redirect_pc = target[63:2];
    tick();
    redirect_valid = 1'b0;
  endtask

  // Fetch must deliver each frame once, in order. The frame lengths 4, 5 and 2 must
  // move the PC by 4, 8 and 4 bytes.
  task automatic test_sequence();
    do_reset();
    expect_frame(64'h00);
    expect_frame(64'h04);
    expect_frame(64'h0C);
    expect_frame(64'h10);
  endtask

  // A frame that Decode does not accept must stay unchanged, and Fetch must send no new
  // request. The next frame comes after Decode accepts the first.
  task automatic test_backpressure();
    logic [63:0]     pc0;
    logic [8*MB-1:0] b0;
    do_reset();
    wait_frame();
    pc0 = frame_pc;
    b0  = frame_bytes;
    for (int i = 0; i < 12; i++) begin
      tick();
      check(frame_valid, "frame_valid dropped while not ready");
      check(frame_pc === pc0 && frame_bytes === b0, "frame changed while not ready");
      check(!imem_req_valid, "new request while a frame is held");
    end
    ack_frame();
    expect_frame(64'h04);
  endtask

  // Fetch must send no request while halt is high. When halt goes low, Fetch starts
  // at the reset PC.
  task automatic test_halt();
    do_reset(1'b1);
    for (int i = 0; i < 20; i++) begin
      check(!imem_req_valid, "request while halt is high");
      tick();
    end
    halt = 1'b0;
    expect_frame(64'h00);
  endtask

  // A redirect in the cycle of a request must discard the reply to that request. The
  // next frame must come from the new address.
  task automatic test_redirect_request();
    do_reset();
    wait_state(ST_REQUEST, -1);
    fire_redirect(64'h40);
    expect_frame(64'h40);
  endtask

  // A redirect while Fetch waits for a reply must discard that reply when it arrives
  // later. The next frame must come from the new address.
  task automatic test_redirect_wait_no_reply();
    do_reset();
    wait_state(ST_WAIT, 0);
    fire_redirect(64'h40);
    expect_frame(64'h40);
  endtask

  // A redirect in the cycle of the reply must discard that reply. The next frame must
  // come from the new address.
  task automatic test_redirect_wait_with_reply();
    do_reset();
    wait_state(ST_WAIT, 1);
    fire_redirect(64'h40);
    expect_frame(64'h40);
  endtask

  // A redirect while a frame waits for Decode must remove that frame at once. The next
  // frame must come from the new address.
  task automatic test_redirect_deliver();
    do_reset();
    wait_frame();
    tick(3);
    fire_redirect(64'h40);
    check(!frame_valid, "frame_valid still high after a redirect in DELIVER");
    expect_frame(64'h40);
  endtask

  // A second redirect in the cycle of the old reply must win. The next frame must come
  // from the second target.
  task automatic test_redirect_drain_with_reply();
    do_reset();
    wait_state(ST_WAIT, 0);
    fire_redirect(64'h40);
    wait_state(ST_DRAIN, 1);
    fire_redirect(64'h80);
    expect_frame(64'h80);
  endtask

  // A second redirect before the old reply arrives must win. Fetch must still discard
  // that one reply.
  task automatic test_double_redirect();
    do_reset();
    wait_state(ST_WAIT, 0);
    fire_redirect(64'h40);
    wait_state(ST_DRAIN, 0);
    fire_redirect(64'h80);
    expect_frame(64'h80);
  endtask

  // The next request must be at the frame address plus the length, rounded up to a
  // multiple of four. A length below 2 must set frame_bad_len.
  task automatic test_length(input int len);
    logic [63:0] expected_next;
    mem[10'h101] = {len[3:0], 4'h1};
    do_reset();
    fire_redirect(64'h100);
    wait_frame();
    check(frame_pc === 64'h100, $sformatf("length %0d: frame_pc %h", len, frame_pc));
    check(frame_bad_len === (len < 2), $sformatf("length %0d: frame_bad_len %b", len, frame_bad_len));
    ack_frame();
    expected_next = 64'h100 + 64'(((len + 3) / 4) * 4);
    check(imem_req_valid, $sformatf("length %0d: no request after the frame", len));
    check(imem_addr === expected_next,
          $sformatf("length %0d: next address %h, expected %h", len, imem_addr, expected_next));
  endtask

  initial begin
    // Each byte has its own value, so a frame from a wrong address is easy to find.
    for (int a = 0; a < 1024; a++) mem[a] = 8'(a * 7 + 3);
    // The first three frames are mov, add and halt, with lengths 4, 5 and 2.
    {mem[0], mem[1], mem[2], mem[3]} = {8'h01, 8'h41, 8'h01, 8'h05};
    {mem[4], mem[5], mem[6], mem[7], mem[8]} = {8'h10, 8'h50, 8'h02, 8'h01, 8'h01};
    {mem[12], mem[13]} = {8'hFF, 8'h20};

    test_sequence();
    test_backpressure();
    test_halt();
    test_redirect_request();
    test_redirect_deliver();
    test_redirect_wait_with_reply();
    // A latency of 1 has no cycle with a request in flight and no reply.
    if (LATENCY > 1) begin
      test_redirect_wait_no_reply();
      test_redirect_drain_with_reply();
    end
    if (LATENCY > 2) test_double_redirect();
    for (int len = 0; len < 16; len++) test_length(len);

    $display("tb_fetch: latency %0d, ready toggle %0d: %0d errors", LATENCY, READY_TOGGLE, errors);
    if (errors != 0) $fatal(1, "tb_fetch failed");
    $finish;
  end

  initial begin
    #5000000;
    $fatal(1, "tb_fetch watchdog");
  end
endmodule
