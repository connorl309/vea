//! Fetch stage for Vea.
//!
//! The PC is always a multiple of four, so Fetch stores only PC[63:2].
//! Fetch reads only the length nibble of a frame to find the next PC, as the
//! pipelined simulator does.
//! Only one memory request is in flight at a time, so replies stay in order.

module vea_fetch #(
  parameter int           MAX_INSN_BYTES = 13,
  parameter logic [63:2]  RESET_PC       = '0
) (
  input  logic clk,
  input  logic rst_n,

  //! The memory returns MAX_INSN_BYTES bytes from imem_addr, with the byte at imem_addr
  //! at the top.
  output logic                        imem_req_valid,
  input  logic                        imem_req_ready,
  output logic [63:0]                 imem_addr,
  input  logic                        imem_rvalid,
  input  logic [8*MAX_INSN_BYTES-1:0] imem_rdata,

  //! Execute drives these. A redirect has priority over all
  //! other events in the same cycle. A branch target must be a multiple of four, so
  //! bits 1:0 are not in the port.
  input  logic         redirect_valid,
  input  logic [63:2]  redirect_pc,

  //! Decode holds this high after it sees a halt
  input  logic halt,

  output logic                        frame_valid,
  input  logic                        frame_ready,
  output logic [63:0]                 frame_pc,
  output logic [8*MAX_INSN_BYTES-1:0] frame_bytes,
  //! The length nibble is below 2, so the frame has no room for an opcode and an opinfo
  //! byte. Decode must fault on this frame.
  output logic                        frame_bad_len
);

  localparam logic [3:0] LEN_MIN = 4'd2;

  typedef enum logic [1:0] {
    S_REQUEST,
    S_WAIT,
    S_DELIVER,
    // A redirect can cancel a request that memory already has. Fetch must discard the
    // reply, or the next frame would come from the old address.
    S_DRAIN
  } state_t;

  state_t state, next_state;
  logic [63:2] pc;
  logic [63:2] frame_pc_word;

  logic [7:4]  opinfo;
  logic [3:0]  len;
  logic [2:0]  len_words;
  logic [63:2] next_pc;
  logic        request_taken;
  logic        accept_reply;

  assign opinfo = imem_rdata[8*MAX_INSN_BYTES-9 -: 4];
  assign len    = opinfo[7:4];

  // The length rounds up to a whole number of four-byte words, as the assembler pads
  // each instruction. The PC then stays a multiple of four.
  assign len_words = {1'b0, len[3:2]} + {2'b0, |len[1:0]};
  assign next_pc   = pc + {59'b0, len_words};

  assign imem_req_valid = (state == S_REQUEST) && !halt;
  assign imem_addr      = {pc, 2'b00};
  assign request_taken  = imem_req_valid && imem_req_ready;
  assign accept_reply   = (state == S_WAIT) && imem_rvalid && !redirect_valid;

  assign frame_valid = (state == S_DELIVER);
  assign frame_pc    = {frame_pc_word, 2'b00};

  always_comb begin
    next_state = state;

    if (redirect_valid) begin
      // A memory request or a reply that is still owed belongs to the old address.
      unique case (state)
        S_REQUEST: next_state = request_taken ? S_DRAIN : S_REQUEST;
        S_WAIT:    next_state = imem_rvalid   ? S_REQUEST : S_DRAIN;
        S_DELIVER: next_state = S_REQUEST;
        S_DRAIN:   next_state = imem_rvalid   ? S_REQUEST : S_DRAIN;
      endcase
    end else begin
      unique case (state)
        S_REQUEST: if (request_taken) next_state = S_WAIT;
        S_WAIT:    if (imem_rvalid)   next_state = S_DELIVER;
        S_DELIVER: if (frame_ready)   next_state = S_REQUEST;
        S_DRAIN:   if (imem_rvalid)   next_state = S_REQUEST;
      endcase
    end
  end

  // The reset is synchronous because the FPGA flip-flops have a synchronous reset input.
  always_ff @(posedge clk) begin
    if (!rst_n) begin
      state <= S_REQUEST;
      pc    <= RESET_PC;
    end else begin
      state <= next_state;
      if (redirect_valid)
        pc <= redirect_pc;
      else if (accept_reply)
        pc <= next_pc;
    end
  end

  // The frame registers have no reset. Decode reads them only when frame_valid is high.
  always_ff @(posedge clk) begin
    if (accept_reply) begin
      frame_pc_word <= pc;
      frame_bytes   <= imem_rdata;
      frame_bad_len <= (len < LEN_MIN);
    end
  end

endmodule
