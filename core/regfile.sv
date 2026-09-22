//! Register file for Vea. It has one write port and two read ports.
//!
//! The read ports have no clock, because Decode needs the value in the same cycle it
//! picks the register.
//!
//! No same-cycle write bypass. Only one instruction is ever in flight, so a read always
//! lands a cycle after the write it needs.

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

  assign rdata_a = regs[raddr_a];
  assign rdata_b = regs[raddr_b];

endmodule
