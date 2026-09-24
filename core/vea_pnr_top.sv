//! Place-and-route harness for vea_core. It is not a system.
//!
//! vea_core has 182 port bits. Most of them are the instruction memory port, and that
//! memory does not exist yet. With no pin constraints, the placer spreads these pins around
//! the whole chip edge. Some pin routes then take more than 4 ns. They also pull Fetch and
//! Decode out of place.
//!
//! This module keeps only the clock and the SPI pins. It drives the instruction port with
//! test data and reads every other output. So place and route sees the whole core and not
//! the pins.
//!
//! The data is pseudo-random, so the core runs garbage. Use this module for timing and
//! area only. vea_board_top holds the real pin map.

module vea_pnr_top (
  input  logic clk,

  //! The SPI pins of the program memory. These are the only pins of a real board that
  //! this harness keeps.
  output logic prog_spi_sck,
  output logic prog_spi_cs_n,
  output logic prog_spi_mosi,
  input  logic prog_spi_miso
);

  localparam int MAX_INSN_BYTES = 13;
  localparam int RDATA_BITS     = 8 * MAX_INSN_BYTES;

  // ---- Reset ---------------------------------------------------------------------------

  // No pin carries the reset. A counter releases it a few cycles after power-on. The
  // flip-flops of the FPGA start at zero after configuration, so it needs no reset.
  // The counter must feed back on itself. A shift register with a constant input becomes a
  // constant in synthesis. The reset then never asserts, and synthesis removes the core.
  logic [3:0] por_count;
  logic       rst_n;

  always_ff @(posedge clk) if (!por_count[3]) por_count <= por_count + 4'd1;

  assign rst_n = por_count[3];

  // ---- Core ----------------------------------------------------------------------------

  logic                  imem_req_valid;
  logic [63:0]           imem_addr;
  logic                  imem_rvalid;
  logic [RDATA_BITS-1:0] imem_rdata;
  logic                  halt, err_illegal, err_unsupported, err_trap, err_unaligned;

  vea_core #(.MAX_INSN_BYTES(MAX_INSN_BYTES)) u_core (
    .clk             (clk),
    .rst_n           (rst_n),
    .imem_req_valid  (imem_req_valid),
    .imem_req_ready  (1'b1),
    .imem_addr       (imem_addr),
    .imem_rvalid     (imem_rvalid),
    .imem_rdata      (imem_rdata),
    .dmem_spi_sck    (prog_spi_sck),
    .dmem_spi_cs_n   (prog_spi_cs_n),
    .dmem_spi_mosi   (prog_spi_mosi),
    .dmem_spi_miso   (prog_spi_miso),
    .halt            (halt),
    .err_illegal     (err_illegal),
    .err_unsupported (err_unsupported),
    .err_trap        (err_trap),
    .err_unaligned   (err_unaligned)
  );

  // ---- Instruction memory --------------------------------------------------------------

  // Like block RAM, this memory is always ready and replies one cycle after a request.
  // Fetch expects exactly one reply for each request.
  always_ff @(posedge clk) imem_rvalid <= imem_req_valid;

  // Constant data would let synthesis remove most of the core, so the data must change.
  // The feedback bit comes from the core outputs. Without a user for them, synthesis would
  // remove the outputs and the logic behind them.
  logic                  feedback, stim_in;
  logic [RDATA_BITS-1:0] stim;

  // An all-zero register would stay zero. The reset puts ones into it. The taps 104, 103,
  // 94 and 93 give a sequence of maximum length.
  assign stim_in = ~rst_n | (stim[103] ^ stim[102] ^ stim[93] ^ stim[92] ^ feedback);

  always_ff @(posedge clk) stim <= {stim[RDATA_BITS-2:0], stim_in};

  assign imem_rdata = stim;

  // ---- Output sink ---------------------------------------------------------------------

  // One output pin would keep these outputs alive. This tree does the same with no pin.
  // Each stage has one LUT, so it does not set the clock speed.
  logic [71:0] sink;
  logic [17:0] fold1;
  logic [19:0] fold1_pad;
  logic [4:0]  fold2;

  assign sink = {3'b000, halt, err_illegal, err_unsupported, err_trap, err_unaligned,
                 imem_addr};
  assign fold1_pad = {2'b00, fold1};

  always_ff @(posedge clk) begin
    for (int i = 0; i < 18; i++) fold1[i] <= ^sink[4*i +: 4];
    for (int i = 0; i < 5; i++)  fold2[i] <= ^fold1_pad[4*i +: 4];
    feedback <= ^fold2;
  end

endmodule
