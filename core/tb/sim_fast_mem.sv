//! Fast stand-in for vea_mem_if, for simulation only.
//!
//! It offers the same req/rvalid port as vea_mem_if (see vea_pkg::mem_req_t). It has no
//! SPI wires and no bit-serial shifting. One request takes one clock cycle, not around
//! 200.
//!
//! Real hardware and synthesis never use this file. See vea_core.sv's SIM_FAST_MEM
//! parameter, which stays 0 by default and picks the real vea_mem_if. A test that
//! checks the SPI wire protocol itself, such as tb_mem_if.sv, must still use the real
//! vea_mem_if.
//!
//! This module clears its memory on reset. Real hardware could not do this (RAM has no
//! bulk clear), but this module never becomes real hardware, so the clear costs
//! nothing real. It also means load_program_file's existing reset pulse (see
//! tb_core.sv) already isolates one program's data from the next, with no extra code.

module vea_sim_fast_mem #(
  parameter int MEM_BYTES = 65536
) (
  input  logic clk,
  input  logic rst_n,

  input  logic               req_valid,
  output logic               req_ready,
  input  vea_pkg::mem_req_t  req,
  output logic               rvalid,
  output logic [63:0]        rdata
);

  logic [7:0] mem [MEM_BYTES];

  // MEM_BYTES is a window into the full 24-bit address space, not all of it. Matches
  // tb_spi_mem.sv's own rule.
  function automatic int unsigned idx(input logic [63:0] a);
    idx = int'(a) % MEM_BYTES;
  endfunction

  // Matches vea_mem_if's own size encoding (execute.sv's SIZE_D/B/H/W).
  function automatic int unsigned width_bytes(input logic [1:0] sz);
    case (sz)
      2'b01:   width_bytes = 1; // B
      2'b10:   width_bytes = 2; // H
      2'b11:   width_bytes = 4; // W
      default: width_bytes = 8; // D
    endcase
  endfunction

  // A function, not inline part-selects: see tb_core.sv's read_frame for why. This file
  // hits the same quirk.
  function automatic logic [63:0] read_word(input logic [63:0] a, input int unsigned n);
    logic [63:0] d;
    d = '0;
    for (int i = 0; i < n; i++) d[8*(n-1-i) +: 8] = mem[idx(a) + i];
    return d;
  endfunction

  assign req_ready = 1'b1; // Always free: no protocol state to wait on.

  int unsigned n;

  always_ff @(posedge clk) begin
    rvalid <= 1'b0;
    if (!rst_n) begin
      for (int i = 0; i < MEM_BYTES; i++) mem[i] <= 8'h00;
    end else if (req_valid) begin
      rvalid <= 1'b1;
      n = width_bytes(req.size);
      if (req.we) begin
        for (int i = 0; i < n; i++)
          mem[idx(req.addr) + i] <= req.wdata[8*(n-1-i) +: 8];
      end else begin
        rdata <= read_word(req.addr, n);
      end
    end
  end

endmodule
