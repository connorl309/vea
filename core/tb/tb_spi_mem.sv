//! Behavioral SPI SRAM for testbenches. Speaks the same protocol as vea_mem_if: an
//! 8-bit command, a 24-bit address, then data bytes most significant byte first. Mem
//! latency is 1 cycle: it never adds a wait state of its own, only the SPI bit shifting
//! itself takes time. Reused by tb_mem_if.sv and tb_core.sv so the protocol is written
//! once.
//!
//! A real chip is clocked by the incoming SCK pin, and this model does the same: every
//! block below triggers on an edge of spi_sck or spi_cs_n, never on the testbench's own
//! clk.

module tb_spi_mem #(
  parameter int MEM_BYTES = 65536
) (
  input  logic spi_sck,
  input  logic spi_cs_n,
  input  logic spi_mosi,
  output logic spi_miso
);

  localparam logic [7:0] CMD_WRITE = 8'h02;

  logic [7:0] mem [MEM_BYTES];

  int          bit_cnt;
  logic [31:0] hdr_sr;
  logic        is_write;
  logic [23:0] addr;
  logic [7:0]  wr_byte;
  logic [7:0]  out_byte;

  assign spi_miso = out_byte[7];

  // MEM_BYTES is a window into the full 24-bit address space, not all of it.
  function automatic int unsigned idx(input logic [23:0] a);
    idx = int'(a) % MEM_BYTES;
  endfunction

  // CS high is this model's reset: one always block, like an RTL block combining a
  // clock and an async reset, so there is one driver for bit_cnt, not two.
  always @(posedge spi_sck or posedge spi_cs_n) begin
    if (spi_cs_n) begin
      bit_cnt <= 0;
    end else begin
      bit_cnt <= bit_cnt + 1;

      if (bit_cnt < 32) begin
        hdr_sr <= {hdr_sr[30:0], spi_mosi};
        if (bit_cnt == 31) begin
          // hdr_sr has not picked up this last bit yet (nonblocking), so decode the
          // complete header from the concatenation directly instead.
          if ({hdr_sr[30:0], spi_mosi}[31:24] == CMD_WRITE) begin
            is_write <= 1'b1;
            addr     <= {hdr_sr[30:0], spi_mosi}[23:0];
          end else begin
            is_write <= 1'b0;
            addr     <= {hdr_sr[30:0], spi_mosi}[23:0];
            out_byte <= mem[idx({hdr_sr[30:0], spi_mosi}[23:0])];
          end
        end
      end else if (is_write) begin
        wr_byte <= {wr_byte[6:0], spi_mosi};
        if (((bit_cnt - 32) & 'h7) == 'h7) begin
          mem[idx(addr)] <= {wr_byte[6:0], spi_mosi};
          addr           <= addr + 24'd1;
        end
      end else begin
        if (((bit_cnt - 32) & 'h7) == 'h7) begin
          addr     <= addr + 24'd1;
          out_byte <= mem[idx(addr + 24'd1)];
        end else begin
          out_byte <= {out_byte[6:0], 1'b0};
        end
      end
    end
  end

  task automatic clear();
    for (int i = 0; i < MEM_BYTES; i++) mem[i] = 8'h00;
  endtask

  task automatic poke(input int a, input logic [7:0] b);
    mem[a] = b;
  endtask

  function automatic logic [7:0] peek(input int a);
    peek = mem[a];
  endfunction

endmodule
