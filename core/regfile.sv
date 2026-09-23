//! Register file for Vea. It has one write port and two read ports.
//!
//! The read ports have no clock, because Decode needs the value in the same cycle it
//! picks the register.
//!
//! The write port bypasses straight to both read ports (see rdata_a and rdata_b below).
//! The next instruction can need a load's or an idx_store's result the same cycle it
//! commits here. A load, or a store's extra value read, makes Execute wait several
//! cycles. Fetch uses that time to stage the next instruction, so that instruction has
//! nothing left to wait for once the wait ends.

module vea_regfile (
  input  logic clk,

  input  logic        wr_en,
  input  logic [4:0]  wr_addr,
  input  logic [63:0] wr_data,

  input  logic [4:0]  raddr_a,
  input  logic [4:0]  raddr_b,
  output logic [63:0] rdata_a,
  output logic [63:0] rdata_b
);

  // Decode faults on a register number of 32 or more. Every 5-bit address is then valid,
  // and no read needs a range check.
  localparam int NUM_REGS = 32;

  logic [63:0] regs [NUM_REGS];

  // The simulator starts with all registers at zero. The same start value lets a program
  // read a register that it did not write, and gives the same result in co-simulation.
  initial begin
    for (int i = 0; i < NUM_REGS; i++) regs[i] = '0;
  end

  // Do not add a reset to this array. LUT RAM has no reset input, and a reset here
  // forces synthesis to use flip-flops instead.
  always_ff @(posedge clk) begin
    if (wr_en) regs[wr_addr] <= wr_data;
  end

  // Write-first bypass. A write and a read of the same register can happen in the same
  // cycle. Decode does not always avoid this case.
  // A load, or an idx_store's extra value read (see decode.sv), can need a result the
  // same cycle that result commits here. Without this mux, that read would get the old
  // value from regs[]. That value is one cycle stale.
  assign rdata_a = (wr_en && wr_addr == raddr_a) ? wr_data : regs[raddr_a];
  assign rdata_b = (wr_en && wr_addr == raddr_b) ? wr_data : regs[raddr_b];

endmodule
