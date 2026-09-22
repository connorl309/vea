//! Memory interface for Vea. It turns the generic load/store port from Execute into
//! SPI SRAM signals. The pin constraints assign these signals to package pins.
//!
//! Stub: the SPI transaction FSM is not built yet.

module vea_mem_if (
  input  logic clk,
  input  logic rst_n,

  //! Same shape as Execute's mem_ port. One request in flight at a time.
  input  logic         req_valid,
  output logic         req_ready,
  input  logic [63:0]  addr,
  input  logic         we,
  input  logic [1:0]   size,
  input  logic [63:0]  wdata,
  output logic         rvalid,
  output logic [63:0]  rdata,

  //! Package pins, assigned to the SPI SRAM in the constraints file.
  output logic spi_sck,
  output logic spi_cs_n,
  output logic spi_mosi,
  input  logic spi_miso
);

  // TODO: SPI transaction FSM (command byte, address phase, data phase).

  assign req_ready = 1'b0;
  assign rvalid    = 1'b0;
  assign rdata     = '0;

  assign spi_sck  = 1'b0;
  assign spi_cs_n = 1'b1;
  assign spi_mosi = 1'b0;

endmodule
