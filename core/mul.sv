//! Pipelined multiplier for Vea.
//!
//! A single-cycle 64x64 multiply is slow. It builds one long add chain. That chain sets
//! the clock speed of the whole core depending on its size, due to how the FPGA chosen
//! for this project maps multiplies into its 18x18 DSP slices.
//!
//! This module splits the multiply into steps. Each step multiplies one slice of A by all
//! of B. It adds the result into a running total. Each step is fast. The multiply takes
//! more cycles instead.
//!
//! The result keeps only the low 64 bits. This matches the plain multiply the ALU had
//! before.

module vea_mul (
  input  logic clk,
  input  logic rst_n,

  //! Start pulse. One pulse per multiply. Execute waits for valid before it sends
  //! another start.
  input  logic         start,
  input  logic [63:0]  a,
  input  logic [63:0]  b,

  output logic         valid,
  output logic [63:0]  result
);

  localparam int NUM_SLICES = 4;
  localparam int SLICE_BITS = 64 / NUM_SLICES;

  typedef enum logic { S_IDLE, S_RUN } state_t;

  state_t                        state, next_state;
  logic [$clog2(NUM_SLICES)-1:0] slice_idx;
  logic                          last_slice;
  logic [63:0]                   a_hold, b_hold;
  logic [63:0]                   acc;

  assign last_slice = slice_idx == NUM_SLICES - 1;

  always_comb begin
    next_state = state;
    unique case (state)
      S_IDLE: if (start)      next_state = S_RUN;
      S_RUN:  if (last_slice) next_state = S_IDLE;
    endcase
  end

  always_ff @(posedge clk) begin
    if (!rst_n) state <= S_IDLE;
    else        state <= next_state;
  end

  //! One slice of A, times all of B, shifted into place. NUM_SLICES cycles add these up
  //! to make the product.
  //!
  //! The shift keeps only the low 64 bits. Cutting a term to 64 bits before the shift
  //! gives the same answer as cutting it after. Both drop the same high bits.
  logic [SLICE_BITS-1:0]  a_slice;
  logic [SLICE_BITS+63:0] partial;
  logic [63:0]            partial_shifted;

  assign a_slice         = a_hold[slice_idx*SLICE_BITS +: SLICE_BITS];
  assign partial         = a_slice * b_hold;
  assign partial_shifted = partial[63:0] << (slice_idx * SLICE_BITS);

  // No reset here. state has a reset. These registers only move once state is in S_IDLE,
  // so a stale value here never reaches anything.
  always_ff @(posedge clk) begin
    if (state == S_IDLE && start) begin
      a_hold    <= a;
      b_hold    <= b;
      acc       <= '0;
      slice_idx <= '0;
    end else if (state == S_RUN) begin
      acc       <= acc + partial_shifted;
      slice_idx <= slice_idx + 1'b1;
    end
  end

  always_ff @(posedge clk) begin
    if (!rst_n) valid <= 1'b0;
    else        valid <= (state == S_RUN) && last_slice;
  end

  // No reset here. valid has a reset. valid gates this register.
  always_ff @(posedge clk) begin
    if ((state == S_RUN) && last_slice) result <= acc + partial_shifted;
  end

endmodule
