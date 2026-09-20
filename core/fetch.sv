//! Fetch unit of the VEA core.
//!
//! The unit gets the instruction bytes from memory.
//! It gets the length of each instruction from the high nibble of byte 1.
//! It sends the bytes to decode.
//! It then calculates the next PC.
//!
//! - Each instruction starts on a 4-byte boundary.
//! - An instruction has 12 bytes or less. The unit reads 12 bytes for each instruction.
//! - The bytes are big endian. Byte 0 is in the top 8 bits.
//! - After a halt instruction, the unit stops.
//!   It starts again on a reset or a redirect.
//! - Memory can take more than 1 cycle to reply, and it does not cancel a request.
//!   After a redirect, the unit waits for the reply of a request in flight and drops it.
//!   Without this, the unit takes the old reply as the new instruction.
//! - Memory gives 1 reply for each request, in order.
//!   Memory must drop its requests in a reset, because the unit does not remember them.

module vea_fetch #(
    parameter [63:0] RESET_PC = 64'h0 //! PC value after a reset
  ) (
    input wire clk, //! Clock. The unit changes state on the rising edge.
    input wire rst, //! Reset. If the signal is 1, the PC goes to RESET_PC.

    input wire redirect_valid, //! If 1, the PC changes to redirect_pc. Execute sets this for a taken branch or jump.
    input wire [63:0] redirect_pc, //! New PC value

    input wire imem_rvalid, //! If 1, imem_rdata has the bytes that the unit asked for.
    input wire [95:0] imem_rdata, //! 12 bytes from memory
    input wire insn_ready, //! If 1, decode takes the instruction in this cycle.

    output logic insn_valid, //! If 1, the insn outputs are valid.
    output logic imem_req, //! If 1, the unit asks memory for the bytes at imem_addr.
    output logic [63:0] imem_addr, //! Address of the first byte to read
    output logic [63:0] insn_pc, //! Address of the instruction
    output logic [95:0] insn_bytes, //! Bytes of the instruction. Decode ignores the bytes after insn_len.
    output logic [3:0] insn_len //! Length of the instruction in bytes. A valid instruction has 2 to 12 bytes. Decode must report an error for other values.
  );

  //! Opcode byte of the halt instruction. The value is the same as in the assembler.
  localparam logic [7:0] OP_HALT = 8'hFF;

  //! States of the fetch unit.
  typedef enum logic [2:0] {
            STATE_ASK_FOR_MEM, //! Ask memory for the bytes at the PC.
            STATE_WAIT_FOR_MEM,    //! Wait for memory to send the bytes.
            STATE_DRAIN_MEM,    //! A redirect came while a request was in flight. Wait for the reply and drop it.
            STATE_SEND_TO_DECODE,    //! Give the bytes to decode.
            STATE_HALT     //! Stop. Decode took a halt instruction.
  } state_t;

  state_t state;
  logic [63:0] pc; //! Address of the current instruction
  logic [95:0] bytes_q; //! Bytes that memory sent
  wire [7:0] opcode = bytes_q[95:88]; //! First byte of the instruction

  //! The next PC is the end of this instruction, rounded up to a multiple of 4.
  wire [63:0] pc_next = (pc + 64'(insn_len) + 64'd3) & ~64'd3;

  //! The unit never sends a request while another one is in flight. So at most 1 request is in flight, and it needs no counter.
  //! A request is in flight after this edge if the unit sends it now, or if its reply has not arrived.
  wire request_in_flight = (state == STATE_ASK_FOR_MEM) ||
       ((state == STATE_WAIT_FOR_MEM || state == STATE_DRAIN_MEM) && !imem_rvalid);

  assign imem_req   = (state == STATE_ASK_FOR_MEM);
  assign imem_addr  = pc;
  assign insn_valid = (state == STATE_SEND_TO_DECODE);
  assign insn_pc    = pc;
  assign insn_bytes = bytes_q;
  assign insn_len   = bytes_q[87:84];

  //! A redirect has priority over each other action, except reset.
  //! It removes the instruction that is in progress.
  always_ff @(posedge clk)
  begin : FetchFsm
    if (rst)
    begin
      pc <= RESET_PC;
      state <= STATE_ASK_FOR_MEM;
    end
    else if (redirect_valid)
    begin
      pc <= redirect_pc;
      state <= request_in_flight ? STATE_DRAIN_MEM : STATE_ASK_FOR_MEM;
    end
    else
    begin
      case (state)
        STATE_ASK_FOR_MEM:
          state <= STATE_WAIT_FOR_MEM;
        STATE_WAIT_FOR_MEM:
          if (imem_rvalid)
          begin
            bytes_q <= imem_rdata;
            state <= STATE_SEND_TO_DECODE;
          end
        STATE_DRAIN_MEM:
          if (imem_rvalid)
            state <= STATE_ASK_FOR_MEM;
        STATE_SEND_TO_DECODE:
          if (insn_ready)
          begin
            pc <= pc_next;
            state <= (opcode == OP_HALT) ? STATE_HALT : STATE_ASK_FOR_MEM;
          end
        STATE_HALT:
          $display("Hit a HALT instruction at pc=%h", pc);
        //! The state register has 3 bits, and 3 values are not states. A bad value must not lock the unit.
        default:
          state <= STATE_ASK_FOR_MEM;
      endcase
    end
  end

endmodule
