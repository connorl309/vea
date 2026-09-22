//! Register file for Vea. It has one write port and two read ports.
//!
//! The read ports have no clock, because Decode needs each value in the cycle in which it
//! finds the register number.
//!
//! Decode does not stall on a register hazard. A read of the register that the write port
//! writes in the same cycle therefore returns the new value. A result that is not at the
//! write port yet is not visible here. The stage that owns this module must forward it.

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

  // The array has no reset. LUT RAM has no reset input, so a reset would move the
  // registers into flip-flops.
  always_ff @(posedge clk) begin
    if (wr_en) regs[wr_addr] <= wr_data;
  end

  logic hit_a, hit_b;

  // The instruction that writes is in the write-back cycle, and the instruction that
  // reads it can be in Decode in the same cycle. The array has the new value only after
  // this clock edge.
  assign hit_a = wr_en && (wr_addr == raddr_a);
  assign hit_b = wr_en && (wr_addr == raddr_b);

  assign rdata_a = hit_a ? wr_data : regs[raddr_a];
  assign rdata_b = hit_b ? wr_data : regs[raddr_b];

endmodule
