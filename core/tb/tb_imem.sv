//! Instruction memory model for testbenches.
//!
//! The model does not cancel a request. A reply for an old request arrives
//! after a redirect, as in a real memory. This way the test can find a fetch
//! unit that takes a stale reply.

module tb_imem #(
    parameter int LATENCY = 1, //! Clock cycles from request to reply. Fetch needs 1 or more.
    parameter int SIZE = 4096
  ) (
    input wire clk,
    input wire req,
    input wire [63:0] addr,
    output logic rvalid,
    output logic [95:0] rdata
  );

  logic [7:0] mem [SIZE];
  logic valid_pipe [LATENCY];
  logic [95:0] data_pipe [LATENCY];

  initial
  begin
    for (int i = 0; i < SIZE; i++)
      mem[i] = 8'h00;
    for (int i = 0; i < LATENCY; i++)
      valid_pipe[i] = 1'b0;
  end

  //! The first "n" bytes of "lit" go to memory at "at". The first byte is the top byte of "lit", as in a hex dump.
  task automatic put(input int at, input logic [95:0] lit, input int n);
    for (int i = 0; i < n; i++)
      mem[at + i] = lit[8 * (n - 1 - i) +: 8];
  endtask

  //! A request outside the model is a testbench error. A wrong address must not give silent zero bytes.
  function automatic logic [95:0] read12(input logic [63:0] a);
    logic [95:0] r;
    r = '0;
    if (a > 64'(SIZE) - 64'd12)
      $fatal(1, "tb_imem: request at %h is outside the memory", a);
    for (int i = 0; i < 12; i++)
      r = {r[87:0], mem[int'(a) + i]};
    return r;
  endfunction

  //! The model reads the bytes in the cycle of the request. A later write to memory does not change the reply.
  always_ff @(posedge clk)
  begin : ReplyPipe
    valid_pipe[0] <= req;
    data_pipe[0] <= req ? read12(addr) : '0;
    for (int i = 1; i < LATENCY; i++)
    begin
      valid_pipe[i] <= valid_pipe[i - 1];
      data_pipe[i] <= data_pipe[i - 1];
    end
  end

  assign rvalid = valid_pipe[LATENCY - 1];
  assign rdata = data_pipe[LATENCY - 1];

endmodule
