// assembler/mod.rs
/**
 * The assembler submodule is responsible for parsing and assembling input
 * source files into their bytecode representation. It enforces syntax and
 * should remove the possibility of non-sim-modified valid programs to not
 * crash the simulation unexpectedly.
*/

/*

                            INSTRUCTION FORMAT

VEA is ***Big Endian***. This means everything will read left-to-right as
is the case with English.

Every instruction in VEA is somewhat-variable-length. The instruction bytes
shall encode the full instruction length, reserved flag bits that impact
per-instruction behavior, register destinations/sources, and possible immediates.
Instructions using immediates shall be truncated as much as possible to keep
byte count small if possible.

This means that, for example, an immediate add with a value of 0x5 should only
be assembled to denote 1 byte of immediate follows, in which case the architecture
(when performing work on this value) will expand immediates into their "corrected"
64-bit equivalent internally. This is so we don't have to worry about size changes
all over structures.

The general format is as follows, listed in big endian byte order:

[   opcode byte    ][   opinfo   ][  rd/rb/imm*  ][  rs1/imm*  ][   rs2/imm*   ]

opinfo is special. In most instructions it is composing the byte of [   payload length[4] | flags[4]    ].
However, for 

The bytes marked with * are optional and presence depends on the instruction itself. For example, a
raw JMP instruction to some hardcoded 64-bit address would take up more byte space than a CALL <reg>,
while a register base + displacement store could use ALL possible bytes.

*/

// What do the bits in flags[4] represent?
// TODO: Determine others
pub const USES_IMMEDIATE: u8 = 0b100;

enum OpType {
    NULL, // NOP and HALT

    RdRs, // rd OP rs
    RdImm, // rd OP imm
    RdRs1Rs2, // rd = rs1 OP rs2
    RdRsImm, // rd = rs OP imm
    
}

#[derive(Debug, Clone)]
pub struct Instruction {
    mnemonic: &'static str,
    opcode: u8,
    plen: u8,
    flags: u8,
}