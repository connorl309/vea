//! Writeback stage for Vea. It commits architectural state: the one write port to the
//! register file, and the redirect to Fetch and Decode.
//!
//! A load, a store and a jump all already resolve their value in Execute, so Writeback
//! has no case of its own for them. It only registers whatever Execute hands it.
//!
//! See tb/tb_writeback.sv for the testbench.

module vea_writeback (
  input  logic clk,
  input  logic rst_n,

  //! Execute drives these. wb_rd and wb_data matter only in a cycle where wb_valid is
  //! high. wb_redirect_pc matters only in a cycle where wb_redirect_valid is high.
  input  logic         wb_valid,
  input  logic [4:0]   wb_rd,
  input  logic [63:0]  wb_data,
  input  logic         wb_redirect_valid,
  input  logic [63:2]  wb_redirect_pc,

  //! One write port to the register file.
  output logic         rf_wr_en,
  output logic [4:0]   rf_wr_addr,
  output logic [63:0]  rf_wr_data,

  //! Fetch and Decode both take this.
  output logic         redirect_valid,
  output logic [63:2]  redirect_pc
);

  always_ff @(posedge clk) begin
    if (!rst_n) begin
      rf_wr_en       <= 1'b0;
      redirect_valid <= 1'b0;
    end else begin
      rf_wr_en       <= wb_valid;
      redirect_valid <= wb_redirect_valid;
    end
  end

  // These registers have no reset. rf_wr_en and redirect_valid gate their own values,
  // so a stale address or target here never reaches anything.
  // rf_wr_data must change only on a write. Execute forwards from it after rf_wr_en
  // goes low. An enable-free register here gives a wrong operand to Execute.
  always_ff @(posedge clk) begin
    if (wb_valid) begin
      rf_wr_addr <= wb_rd;
      rf_wr_data <= wb_data;
    end
    if (wb_redirect_valid) redirect_pc <= wb_redirect_pc;
  end

endmodule
