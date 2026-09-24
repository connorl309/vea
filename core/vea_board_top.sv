//! Board top level for the Vea core on the LFE5U-12F-6BG256C.
//!
//! This module holds every pin of the board. vea_board.lpf maps the ports to balls.
//!
//! The core does not run from the oscillator pin. vea_pll multiplies the 25 MHz
//! oscillator up to the core clock, and the core stays in reset until the PLL locks.
//!
//! The board has two SPI memories. Each memory has its own bus, so each takes a new
//! image without the other.
//!
//!   * The boot flash holds the bitstream. It is on the master SPI port. Only the
//!     configuration controller reads it, so this module has no port for it.
//!   * The program memory holds the user program. It is on usual fabric I/O. The
//!     vea_mem_if in the core drives it.
//!
//! The core reads the program memory with loads. It cannot write to it. vea_mem_if sends
//! no write enable command before a write command. The memory ignores a write command
//! without one, so a store has no effect.
//!
//! Fetch has no path to the program memory yet. See STUB_IMEM below.

module vea_board_top #(
  parameter int MAX_INSN_BYTES = 13,

  //! 1 drives the instruction port with a test pattern, because Fetch has no path to the
  //! program memory. A constant on that port lets synthesis remove the core, so the
  //! pattern must change on each cycle. The core then runs invalid instructions. Use
  //! this mode for place and route, for pin checks and for power checks only.
  //! Set this parameter to 0 when Fetch reads the program memory.
  parameter bit STUB_IMEM = 1'b1
) (
  //! The board oscillator, 25 MHz. vea_board.lpf puts this on a PLL input ball.
  input  logic clk_25mhz,

  //! The SPI memory with the user program.
  output logic prog_spi_sck,
  output logic prog_spi_cs_n,
  output logic prog_spi_mosi,
  input  logic prog_spi_miso,

  //! Status indicators. The LEDs go to the supply through a resistor, so a low level
  //! makes an LED come on.
  output logic [4:0] led_n
);

  localparam int RDATA_BITS = 8 * MAX_INSN_BYTES;

  // ---- Clock ---------------------------------------------------------------------------

  logic clk_core, pll_locked;

  vea_pll u_pll (
    .clk_in   (clk_25mhz),
    .clk_core (clk_core),
    .locked   (pll_locked)
  );

  // ---- Reset ---------------------------------------------------------------------------

  // No pin carries the reset. The counter releases the core some cycles after the PLL
  // locks. The flip-flops of the FPGA start at zero after configuration, so the counter
  // needs no reset of its own.
  logic [3:0] por_count;
  logic       rst_n;

  always_ff @(posedge clk_core) begin
    if (!pll_locked)         por_count <= 4'd0;
    else if (!por_count[3])  por_count <= por_count + 4'd1;
  end

  assign rst_n = por_count[3];

  // ---- Input synchronizer ----------------------------------------------------------------

  // prog_spi_miso comes from another chip and has no relation to the core clock. Two
  // flip-flops make the level safe for the core. vea_mem_if does not do this itself,
  // because simulation has no metastability to model.
  logic miso_meta, miso_sync;

  always_ff @(posedge clk_core) begin
    miso_meta <= prog_spi_miso;
    miso_sync <= miso_meta;
  end

  // ---- Core ------------------------------------------------------------------------------

  logic                  imem_req_valid;
  logic [63:0]           imem_addr;
  logic                  imem_rvalid;
  logic [RDATA_BITS-1:0] imem_rdata;
  logic                  halt, err_illegal, err_unsupported, err_trap, err_unaligned;

  // The data port of the core is the only SPI master, so it owns the program memory bus.
  vea_core #(.MAX_INSN_BYTES(MAX_INSN_BYTES)) u_core (
    .clk             (clk_core),
    .rst_n           (rst_n),
    .imem_req_valid  (imem_req_valid),
    .imem_req_ready  (1'b1),
    .imem_addr       (imem_addr),
    .imem_rvalid     (imem_rvalid),
    .imem_rdata      (imem_rdata),
    .dmem_spi_sck    (prog_spi_sck),
    .dmem_spi_cs_n   (prog_spi_cs_n),
    .dmem_spi_mosi   (prog_spi_mosi),
    .dmem_spi_miso   (miso_sync),
    .halt            (halt),
    .err_illegal     (err_illegal),
    .err_unsupported (err_unsupported),
    .err_trap        (err_trap),
    .err_unaligned   (err_unaligned)
  );

  assign led_n = ~{halt, err_illegal, err_unsupported, err_trap, err_unaligned};

  // ---- Instruction port ---------------------------------------------------------------

  // TODO: icache. Replace this block with a program memory reader. Fetch and the data
  // port then share the one program memory bus, so that reader needs an arbiter.
  generate
    if (STUB_IMEM) begin : g_stub_imem
      // This memory answers one cycle after a request, as block RAM does. Fetch expects
      // exactly one reply for each request.
      always_ff @(posedge clk_core) imem_rvalid <= imem_req_valid;

      // The taps 104, 103, 94 and 93 give a sequence of maximum length. An all-zero
      // register stays zero, so the reset puts ones into it.
      logic                  stim_in;
      logic [RDATA_BITS-1:0] stim;

      assign stim_in = ~rst_n | (stim[103] ^ stim[102] ^ stim[93] ^ stim[92]);

      always_ff @(posedge clk_core) stim <= {stim[RDATA_BITS-2:0], stim_in};

      assign imem_rdata = stim;
    end else begin : g_no_imem
      assign imem_rvalid = 1'b0;
      assign imem_rdata  = '0;
    end
  endgenerate

endmodule
