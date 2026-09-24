//! Memory interface for Vea. It turns the generic load/store port from Execute into a
//! byte-addressed SPI memory transaction: an 8-bit command, then a 24-bit address, then
//! the data bytes, most significant byte first. That matches the big-endian byte order
//! vea_sim's own memory already uses, so a multi-byte load or store lines up with it.
//!
//! SPI mode 0, most significant bit first. One SPI bit takes two clk cycles: sck low
//! while mosi is set up, sck high while the far side samples it (and this side samples
//! miso, for a read).
//!
//! READ and WRITE use the opcodes of the usual 25-series SPI memories. The program memory
//! on the board is such a part.
//!
//! spi_miso has no synchronizer here. Simulation has no metastability to model.
//! vea_board_top adds the synchronizer.

module vea_mem_if (
  input  logic clk,
  input  logic rst_n,

  //! One request in flight at a time. See vea_pkg for the fields of req.
  input  logic              req_valid,
  output logic              req_ready,
  input  vea_pkg::mem_req_t req,
  output logic              rvalid,
  output logic [63:0]       rdata,

  //! Package pins. vea_board.lpf assigns them to the program memory.
  output logic spi_sck,
  output logic spi_cs_n,
  output logic spi_mosi,
  input  logic spi_miso
);

  localparam logic [7:0] CMD_READ  = 8'h03;
  localparam logic [7:0] CMD_WRITE = 8'h02;
  localparam logic [6:0] HDR_BITS  = 7'd32; // 8-bit command + 24-bit address

  typedef enum logic [1:0] {
    S_IDLE,
    S_HDR,
    S_DATA,
    S_DONE
  } state_t;

  state_t state, next_state;

  // Bit timing: sck stays low while phase is 0 (mosi is set up here, from the value
  // already sitting at the top of the shift register), and goes high while phase is 1
  // (the sample edge: the far side reads mosi, and this side reads miso).
  logic phase;

  logic        we_q;
  logic [6:0]  data_bits;   // 8, 16, 32 or 64, set from req.size at the start
  logic [6:0]  bits_left;
  logic [31:0] hdr_sr;
  logic [63:0] data_sr;

  // Only depends on its own input, so it stays clear of Yosys's limits on functions
  // that read module signals.
  function automatic logic [6:0] size_bits(input logic [1:0] sz);
    case (sz)
      2'b01:   size_bits = 7'd8;  // SIZE_B
      2'b10:   size_bits = 7'd16; // SIZE_H
      2'b11:   size_bits = 7'd32; // SIZE_W
      default: size_bits = 7'd64; // SIZE_D
    endcase
  endfunction

  assign req_ready = (state == S_IDLE);
  assign rvalid     = (state == S_DONE);
  assign rdata      = data_sr;

  assign spi_cs_n = (state == S_IDLE) || (state == S_DONE);
  assign spi_sck  = (state == S_HDR || state == S_DATA) && phase;
  assign spi_mosi = (state == S_HDR) ? hdr_sr[31] : (we_q && data_sr[63]);

  always_comb begin
    next_state = state;
    unique case (state)
      S_IDLE: if (req_valid)                 next_state = S_HDR;
      S_HDR:  if (phase && bits_left == 7'd1) next_state = S_DATA;
      S_DATA: if (phase && bits_left == 7'd1) next_state = S_DONE;
      S_DONE:                                 next_state = S_IDLE;
    endcase
  end

  always_ff @(posedge clk) begin
    if (!rst_n) begin
      state <= S_IDLE;
      phase <= 1'b0;
    end else begin
      state <= next_state;

      unique case (state)
        S_IDLE: begin
          phase <= 1'b0;
          if (req_valid) begin
            we_q      <= req.we;
            data_bits <= size_bits(req.size);
            hdr_sr    <= {req.we ? CMD_WRITE : CMD_READ, req.addr[23:0]};
            // Left-justified: a plain shift-out of the top data_bits bits then reads
            // off the low width bytes of wdata, most significant byte first, matching
            // memory.rs's own truncate-and-serialize rule for a narrow store.
            data_sr   <= req.we ? (req.wdata << (7'd64 - size_bits(req.size))) : '0;
            bits_left <= HDR_BITS;
          end
        end

        S_HDR: begin
          if (!phase) begin
            phase <= 1'b1;
          end else begin
            phase  <= 1'b0;
            hdr_sr <= hdr_sr << 1;
            if (bits_left == 7'd1) bits_left <= data_bits;
            else                   bits_left <= bits_left - 7'd1;
          end
        end

        S_DATA: begin
          if (!phase) begin
            // Rising edge: the sample point. miso was set up by the far side during
            // the phase=0 half that just ended, so it is safe to capture now, before
            // this side's own falling-edge shift changes anything.
            phase <= 1'b1;
            if (!we_q) data_sr <= {data_sr[62:0], spi_miso};
          end else begin
            // Falling edge: advance to the next bit, ready for the far side to sample
            // on the rising edge to come.
            phase     <= 1'b0;
            bits_left <= bits_left - 7'd1;
            if (we_q) data_sr <= data_sr << 1;
          end
        end

        default: ; // S_DONE returns to S_IDLE on its own; nothing to update here.
      endcase
    end
  end

endmodule
