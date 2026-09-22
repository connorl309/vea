//! Testbench for vea_mem_if. See ../mem_if.sv for the module. tb_spi_mem.sv plays the
//! SPI SRAM on the other end of the pins.

module tb_mem_if #(
  parameter int SEED = 0
);
  localparam logic [1:0] SIZE_D = 2'b00, SIZE_B = 2'b01, SIZE_H = 2'b10, SIZE_W = 2'b11;

  logic clk   = 1'b0;
  logic rst_n = 1'b0;
  always #5 clk <= ~clk;

  logic              req_valid = 1'b0;
  logic              req_ready;
  vea_pkg::mem_req_t req;
  logic              rvalid;
  logic [63:0]       rdata;

  logic spi_sck, spi_cs_n, spi_mosi, spi_miso;

  vea_mem_if dut (.*);
  tb_spi_mem #(.MEM_BYTES(4096)) slave (.*);

  int errors = 0;
  int checks = 0;

  task automatic check64(input logic [63:0] got, input logic [63:0] want, input string what);
    checks++;
    if (got !== want) begin
      errors++;
      if (errors <= 40) $display("FAIL: tb_mem_if: %s: got %h, expected %h", what, got, want);
    end
  endtask

  task automatic tick(input int n = 1);
    repeat (n) @(posedge clk);
    #1;
  endtask

  task automatic do_reset();
    rst_n     = 1'b0;
    req_valid = 1'b0;
    slave.clear();
    tick(3);
    rst_n = 1'b1;
  endtask

  function automatic int unsigned size_bytes(input logic [1:0] sz);
    case (sz)
      SIZE_B:  size_bytes = 1;
      SIZE_H:  size_bytes = 2;
      SIZE_W:  size_bytes = 4;
      default: size_bytes = 8;
    endcase
  endfunction

  // Big endian: addr holds the most significant of the size_bytes(sz) bytes, matching
  // vea_sim's memory.rs.
  function automatic logic [63:0] low_bytes(input logic [63:0] v, input logic [1:0] sz);
    case (sz)
      SIZE_B:  low_bytes = {56'b0, v[7:0]};
      SIZE_H:  low_bytes = {48'b0, v[15:0]};
      SIZE_W:  low_bytes = {32'b0, v[31:0]};
      default: low_bytes = v;
    endcase
  endfunction

  // Guard against a stuck req_ready/rvalid hanging the test instead of just failing it.
  task automatic wait_ready();
    int guard;
    guard = 0;
    while (!req_ready && guard < 20) begin
      tick();
      guard++;
    end
    check64(64'(req_ready), 64'b1, "req_ready never asserted");
  endtask

  task automatic wait_rvalid();
    int guard;
    guard = 0;
    while (!rvalid && guard < 400) begin
      tick();
      guard++;
    end
    check64(64'(rvalid), 64'b1, "rvalid never asserted");
  endtask

  task automatic do_write(input logic [63:0] addr, input logic [1:0] sz, input logic [63:0] val);
    wait_ready();
    req_valid = 1'b1;
    req       = '{addr: addr, we: 1'b1, size: sz, wdata: val};
    tick();
    req_valid = 1'b0;
    wait_rvalid();
    tick();
  endtask

  task automatic do_read(input logic [63:0] addr, input logic [1:0] sz, output logic [63:0] val);
    wait_ready();
    req_valid = 1'b1;
    req       = '{addr: addr, we: 1'b0, size: sz, wdata: '0};
    tick();
    req_valid = 1'b0;
    wait_rvalid();
    val = rdata;
    tick();
  endtask

  // A store, then a direct peek of the slave's bytes: address a holds the most
  // significant byte, matching memory.rs's write().
  task automatic test_write(input string name, input logic [63:0] addr, input logic [1:0] sz,
                             input logic [63:0] val);
    logic [63:0] want, got;
    int          n;

    n    = size_bytes(sz);
    want = low_bytes(val, sz);
    do_write(addr, sz, val);

    got = '0;
    for (int i = 0; i < n; i++) got = {got[55:0], slave.peek(int'(addr) + i)};
    check64(got, want, {name, ": bytes written, big endian"});
  endtask

  // A direct poke of the slave's bytes, then a load: address a holds the most
  // significant byte, matching memory.rs's read().
  task automatic test_read(input string name, input logic [63:0] addr, input logic [1:0] sz,
                            input logic [63:0] val);
    logic [63:0] want, got;
    int          n;

    n    = size_bytes(sz);
    want = low_bytes(val, sz);
    for (int i = 0; i < n; i++) slave.poke(int'(addr) + i, want[8*(n-1-i) +: 8]);

    do_read(addr, sz, got);
    check64(got, want, {name, ": value read back"});
  endtask

  task automatic test_sizes_directed();
    logic [1:0]  sizes [4];
    logic [63:0] vals  [6];

    sizes = '{SIZE_B, SIZE_H, SIZE_W, SIZE_D};
    vals  = '{64'h0, 64'h1, 64'hFFFF_FFFF_FFFF_FFFF, 64'hDEAD_BEEF_CAFE_F00D,
              64'h8000_0000_0000_0000, 64'h7FFF_FFFF_FFFF_FFFF};

    for (int s = 0; s < 4; s++) begin
      for (int v = 0; v < 6; v++) begin
        test_write($sformatf("write size %0d val %0d", s, v), 64'(64 + s * 64 + v * 8),
                   sizes[s], vals[v]);
        test_read($sformatf("read size %0d val %0d", s, v), 64'(2048 + s * 64 + v * 8),
                   sizes[s], vals[v]);
      end
    end
  endtask

  task automatic test_sizes_random(input int n);
    logic [1:0]  sizes [4];
    logic [1:0]  sz;
    logic [63:0] addr, val;

    sizes = '{SIZE_B, SIZE_H, SIZE_W, SIZE_D};

    for (int i = 0; i < n; i++) begin
      sz   = sizes[$urandom_range(0, 3)];
      addr = 64'(4096 + i * 16);
      val  = {$urandom, $urandom};
      test_write($sformatf("random write %0d", i), addr, sz, val);
      test_read($sformatf("random read %0d", i), addr + 64'd4096, sz, val);
    end
  endtask

  // req_ready must drop as soon as a request is accepted, and must not return until
  // the whole SPI transaction, header and data, has actually finished.
  task automatic test_single_in_flight();
    logic [63:0] got;

    req_valid = 1'b1;
    req       = '{addr: 64'd8192, we: 1'b1, size: SIZE_D, wdata: 64'hAAAA_BBBB_CCCC_DDDD};
    tick();
    req_valid = 1'b0;

    for (int c = 0; c < 5; c++) begin
      check64(64'(req_ready), 64'b0, "req_ready stays low mid-transaction");
      tick();
    end
    wait_rvalid();
    tick();

    do_read(64'd8192, SIZE_D, got);
    check64(got, 64'hAAAA_BBBB_CCCC_DDDD, "single in flight: readback after the write");
  endtask

  initial begin
    process::self().srandom(SEED);
    $display("tb_mem_if: seed %0d", SEED);
    do_reset();
    $display("tb_mem_if: test_sizes_directed");
    test_sizes_directed();
    $display("tb_mem_if: test_sizes_random");
    test_sizes_random(200);
    $display("tb_mem_if: test_single_in_flight");
    test_single_in_flight();

    $display("tb_mem_if: %0d checks, %0d errors", checks, errors);
    if (errors != 0) $fatal(1, "tb_mem_if failed");
    $finish;
  end

  initial begin
    #20_000_000;
    $fatal(1, "tb_mem_if watchdog");
  end
endmodule
