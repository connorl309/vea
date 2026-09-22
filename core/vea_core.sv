//! Top level for a single Vea core.
//!
//! The instruction memory has no interface module yet. TODO: icache

module vea_core #(
  parameter int          MAX_INSN_BYTES = 13,
  parameter logic [63:2] RESET_PC       = '0
) (
  input  logic clk,
  input  logic rst_n,

  //! Same shape as vea_fetch's imem port.
  output logic                        imem_req_valid,
  input  logic                        imem_req_ready,
  output logic [63:0]                 imem_addr,
  input  logic                        imem_rvalid,
  input  logic [8*MAX_INSN_BYTES-1:0] imem_rdata,

  //! Data memory pins, assigned to the SPI SRAM in the constraints file.
  output logic dmem_spi_sck,
  output logic dmem_spi_cs_n,
  output logic dmem_spi_mosi,
  input  logic dmem_spi_miso,

  //! Status for board-level indicators, for example LEDs. Each latches until reset.
  output logic halt,
  output logic err_illegal,
  output logic err_unsupported,
  output logic err_trap,
  output logic err_unaligned
);

  // ---- Cycle counter ---------------------------------------------------------------

  logic [63:0] cycles;

  always_ff @(posedge clk) begin
    if (!rst_n) cycles <= '0;
    else        cycles <= cycles + 1;
  end

  // ---- Fetch <-> Decode ----------------------------------------------------------------

  logic                        frame_valid, frame_ready, frame_bad_len;
  logic [63:0]                 frame_pc;
  logic [8*MAX_INSN_BYTES-1:0] frame_bytes;

  // ---- Decode <-> Register file ---------------------------------------------------------

  logic [4:0]  rf_raddr_a, rf_raddr_b;
  logic [63:0] rf_rdata_a, rf_rdata_b;

  // ---- Decode <-> Execute ----------------------------------------------------------------

  logic        ex_valid, ex_ready;
  logic [63:0] ex_pc, ex_a, ex_b, ex_c;
  logic [3:0]  ex_alu_op;
  logic [4:0]  ex_rd;
  logic        ex_wr_en, ex_is_branch;
  logic [2:0]  ex_pred;
  logic        ex_is_load, ex_is_store;
  logic [1:0]  ex_mem_size;
  logic        ex_mem_sext, ex_is_trap, ex_is_halt, ex_illegal;

  // ---- Execute <-> Writeback --------------------------------------------------------------

  logic        wb_valid;
  logic [4:0]  wb_rd;
  logic [63:0] wb_data;
  logic        wb_redirect_valid;
  logic [63:2] wb_redirect_pc;

  // ---- Writeback <-> Register file, Fetch, Decode ------------------------------------------

  logic        rf_wr_en;
  logic [4:0]  rf_wr_addr;
  logic [63:0] rf_wr_data;
  logic        redirect_valid;
  logic [63:2] redirect_pc;

  // ---- Execute <-> Data memory -------------------------------------------------------------

  logic               dmem_req_valid, dmem_req_ready, dmem_rvalid;
  logic [63:0]        dmem_rdata;
  vea_pkg::mem_req_t  dmem_req;

  vea_fetch #(
    .MAX_INSN_BYTES (MAX_INSN_BYTES),
    .RESET_PC       (RESET_PC)
  ) u_fetch (
    .clk            (clk),
    .rst_n          (rst_n),
    .imem_req_valid (imem_req_valid),
    .imem_req_ready (imem_req_ready),
    .imem_addr      (imem_addr),
    .imem_rvalid    (imem_rvalid),
    .imem_rdata     (imem_rdata),
    .redirect_valid (redirect_valid),
    .redirect_pc    (redirect_pc),
    .halt           (halt),
    .frame_valid    (frame_valid),
    .frame_ready    (frame_ready),
    .frame_pc       (frame_pc),
    .frame_bytes    (frame_bytes),
    .frame_bad_len  (frame_bad_len)
  );

  vea_decode #(
    .MAX_INSN_BYTES (MAX_INSN_BYTES)
  ) u_decode (
    .clk            (clk),
    .rst_n          (rst_n),
    .frame_valid    (frame_valid),
    .frame_ready    (frame_ready),
    .frame_pc       (frame_pc),
    .frame_bytes    (frame_bytes),
    .frame_bad_len  (frame_bad_len),
    .redirect_valid (redirect_valid),
    .halt           (halt),
    .rf_raddr_a     (rf_raddr_a),
    .rf_raddr_b     (rf_raddr_b),
    .rf_rdata_a     (rf_rdata_a),
    .rf_rdata_b     (rf_rdata_b),
    .ex_valid       (ex_valid),
    .ex_ready       (ex_ready),
    .ex_pc          (ex_pc),
    .ex_alu_op      (ex_alu_op),
    .ex_a           (ex_a),
    .ex_b           (ex_b),
    .ex_c           (ex_c),
    .ex_rd          (ex_rd),
    .ex_wr_en       (ex_wr_en),
    .ex_is_branch   (ex_is_branch),
    .ex_pred        (ex_pred),
    .ex_is_load     (ex_is_load),
    .ex_is_store    (ex_is_store),
    .ex_mem_size    (ex_mem_size),
    .ex_mem_sext    (ex_mem_sext),
    .ex_is_trap     (ex_is_trap),
    .ex_is_halt     (ex_is_halt),
    .ex_illegal     (ex_illegal)
  );

  vea_execute u_execute (
    .clk               (clk),
    .rst_n             (rst_n),
    .ex_valid          (ex_valid),
    .ex_pc             (ex_pc),
    .ex_alu_op         (ex_alu_op),
    .ex_a              (ex_a),
    .ex_b              (ex_b),
    .ex_c              (ex_c),
    .ex_rd             (ex_rd),
    .ex_wr_en          (ex_wr_en),
    .ex_is_branch      (ex_is_branch),
    .ex_pred           (ex_pred),
    .ex_is_load        (ex_is_load),
    .ex_is_store       (ex_is_store),
    .ex_mem_size       (ex_mem_size),
    .ex_mem_sext       (ex_mem_sext),
    .ex_is_trap        (ex_is_trap),
    .ex_is_halt        (ex_is_halt),
    .ex_illegal        (ex_illegal),
    .ex_ready          (ex_ready),
    .wb_valid          (wb_valid),
    .wb_rd             (wb_rd),
    .wb_data           (wb_data),
    .wb_redirect_valid (wb_redirect_valid),
    .wb_redirect_pc    (wb_redirect_pc),
    .err_illegal       (err_illegal),
    .err_unsupported   (err_unsupported),
    .err_trap          (err_trap),
    .err_unaligned     (err_unaligned),
    .mem_req_valid     (dmem_req_valid),
    .mem_req_ready     (dmem_req_ready),
    .mem_req           (dmem_req),
    .mem_rvalid        (dmem_rvalid),
    .mem_rdata         (dmem_rdata)
  );

  vea_writeback u_writeback (
    .clk               (clk),
    .rst_n             (rst_n),
    .wb_valid          (wb_valid),
    .wb_rd             (wb_rd),
    .wb_data           (wb_data),
    .wb_redirect_valid (wb_redirect_valid),
    .wb_redirect_pc    (wb_redirect_pc),
    .rf_wr_en          (rf_wr_en),
    .rf_wr_addr        (rf_wr_addr),
    .rf_wr_data        (rf_wr_data),
    .redirect_valid    (redirect_valid),
    .redirect_pc       (redirect_pc)
  );

  vea_regfile u_regfile (
    .clk     (clk),
    .wr_en   (rf_wr_en),
    .wr_addr (rf_wr_addr),
    .wr_data (rf_wr_data),
    .raddr_a (rf_raddr_a),
    .raddr_b (rf_raddr_b),
    .rdata_a (rf_rdata_a),
    .rdata_b (rf_rdata_b)
  );

  vea_mem_if u_mem_if (
    .clk       (clk),
    .rst_n     (rst_n),
    .req_valid (dmem_req_valid),
    .req_ready (dmem_req_ready),
    .req       (dmem_req),
    .rvalid    (dmem_rvalid),
    .rdata     (dmem_rdata),
    .spi_sck   (dmem_spi_sck),
    .spi_cs_n  (dmem_spi_cs_n),
    .spi_mosi  (dmem_spi_mosi),
    .spi_miso  (dmem_spi_miso)
  );

endmodule
