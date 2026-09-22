//! Shared types for Vea.
//!
//! mem_req_t bundles the fields of a data memory request into one wire between Execute
//! and vea_mem_if, in place of four separate wires at vea_core.
//!
//! No file in this design writes an `import` of this package. Yosys 0.33 does not parse
//! it; a fully qualified name such as vea_pkg::mem_req_t works everywhere instead.

package vea_pkg;

  typedef struct packed {
    logic [63:0] addr;
    logic        we;
    logic [1:0]  size;
    logic [63:0] wdata;
  } mem_req_t;

endpackage
