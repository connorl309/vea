//! Pipelined multiplier for Vea.
//!
//! The result keeps only the low 64 bits. This matches the plain multiply the ALU had
//! before.
//!
//! The operands split into four 16-bit limbs. A limb product fits in one 18x18 DSP.
//! Products with a weight of 64 bits or more do not change the low 64 bits. The module
//! does not make them. As such, ten DSPs are necessary, not sixteen.
//!
//! Each stage has one DSP or one carry chain between registers.

module vea_mul (
  input  logic clk,
  input  logic rst_n,

  //! Start pulse. One pulse per multiply.
  input  logic         start,
  input  logic [63:0]  a,
  input  logic [63:0]  b,

  //! A one-cycle pulse. The result is correct only on this cycle.
  output logic         valid,
  output logic [63:0]  result
);

  // ---- Stage 1: operands -----------------------------------------------------------

  // Execute's forward mux gives a and b to the ALU too. A load with no enable puts
  // this register on that path on every cycle. The enable puts it there only on a
  // multiply.
  logic [15:0] a0, a1, a2, a3;
  logic [15:0] b0, b1, b2, b3;

  always_ff @(posedge clk) begin
    if (start) begin
      {a3, a2, a1, a0} <= a;
      {b3, b2, b1, b0} <= b;
    end
  end

  // ---- Stage 2: limb products --------------------------------------------------------

  // The name pIJ is aI times bJ. Its weight is 16 * (I + J) bits.
  logic [31:0] p00, p01, p10, p02, p11, p20;
  // A weight of 48 leaves 16 bits below bit 64.
  logic [15:0] p03, p12, p21, p30;

  always_ff @(posedge clk) begin
    p00 <= a0 * b0;
    p01 <= a0 * b1;
    p10 <= a1 * b0;
    p02 <= a0 * b2;
    p11 <= a1 * b1;
    p20 <= a2 * b0;
    p03 <= 16'(a0 * b3);
    p12 <= 16'(a1 * b2);
    p21 <= 16'(a2 * b1);
    p30 <= 16'(a3 * b0);
  end

  // ---- Stages 3 to 5: adder tree -----------------------------------------------------

  // Products that do not overlap share one row. This gives seven rows, not ten. The
  // tree then needs three levels, not four.
  logic [63:0] r0, r1, r2, r3, r4, r5, r6;

  assign r0 = {p11,       p00};
  assign r1 = {p03,       p01, 16'b0};
  assign r2 = {p30,       p10, 16'b0};
  assign r3 = {p02,       32'b0};
  assign r4 = {p20,       32'b0};
  assign r5 = {p12,       48'b0};
  assign r6 = {p21,       48'b0};

  logic [63:0] s0, s1, s2, s3;
  logic [63:0] t0, t1;

  always_ff @(posedge clk) begin
    s0 <= r0 + r1;
    s1 <= r2 + r3;
    s2 <= r4 + r5;
    s3 <= r6;

    t0 <= s0 + s1;
    t1 <= s2 + s3;

    result <= t0 + t1;
  end

  // ---- Valid -------------------------------------------------------------------------

  // No data register has a reset. Stage 1 has an enable; the other stages do not, and
  // hold whatever stage 1 last gave them. This bit identifies the data of a real
  // multiply, at whichever stage it has reached. Only this bit needs a reset.
  logic [4:0] stage_valid;

  always_ff @(posedge clk) begin
    if (!rst_n) stage_valid <= '0;
    else        stage_valid <= {stage_valid[3:0], start};
  end

  assign valid = stage_valid[4];

endmodule
