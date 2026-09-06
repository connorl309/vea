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

opinfo is special. In all instructions it is composing the byte of [   payload length[4] | flags[4]    ].
Other interpretations are left open as this project develops.

The bytes marked with * are optional and presence depends on the instruction itself. For example, a
raw JMP instruction to some hardcoded 64-bit address would take up more byte space than a CALL <reg>,
while a register base + displacement store could use ALL possible bytes.

*/

use crate::isa::*;

// What do the bits in flags[4] represent?
pub const OPINFO_FLAG_ALSO_IMMEDIATE: u8 = 0b0001;

// The branch instruction has a single opcode; its flags[4] nibble carries a
// BR_* predicate selector that is evaluated against the live ConditionCodes
// (Z = ZERO, N = NEG, C = CARRY, V = OVERFLOW).
//
// Signedness is already resolved by the compare that set the codes -- `cmp` for
// unsigned, `cmp.s` for signed -- and each is required to leave the codes such
// that the relational predicates below hold directly. That keeps one relational
// branch set instead of separate signed/unsigned ladders.
pub const BR_ALWAYS: u8 = 0x0; // b     : unconditional
pub const BR_EQ: u8     = 0x1; // beq   : Z
pub const BR_NE: u8     = 0x2; // bne   : !Z
pub const BR_LT: u8     = 0x3; // blt   : N != V
pub const BR_GE: u8     = 0x4; // bgte  : N == V
pub const BR_GT: u8     = 0x5; // bgt   : !Z && (N == V)
pub const BR_LE: u8     = 0x6; // blte  : Z || (N != V)

#[derive(Debug, Clone, Copy)]
enum Op {
    NULL, // NOP and HALT

    R__OR__I,
    R_R__OR__R_I,
    R_R_R__OR__R_R_IMM, // depending on flags, can be either r = r OP r, *or* r = r OP imm
}

// The basic instruction format struct.
#[derive(Debug, Clone)]
pub struct InstructionFormat {
    mnemonic: &'static str,
    opcode: u8,
    inst_type: Op,
    flags: u8
}
// Quick helper for making new instructions...
const fn row(mnemonic: &'static str, opcode: u8, inst_type: Op, flags: u8) -> InstructionFormat {
    InstructionFormat { mnemonic, opcode, inst_type, flags }
}

// Our actual list of instruction formats
pub const INSTRUCTIONS: &[InstructionFormat] = &[
    // Processor level stuff
    row("nop",  0x00, Op::NULL, 0),
    row("halt", 0xFF, Op::NULL, 0),

    // Move
    // rd = rs, OR
    // rd = imm
    row("mov",  0x01,   Op::R_R__OR__R_I, 0),

    // ALU
    // rd = a OP b, where
    // a = rs1
    // b = rs2 or imm
    row("add",  0x10,   Op::R_R_R__OR__R_R_IMM, 0),
    row("sub",  0x11,   Op::R_R_R__OR__R_R_IMM, 0),
    row("and",  0x12,   Op::R_R_R__OR__R_R_IMM, 0),
    row("or",   0x13,   Op::R_R_R__OR__R_R_IMM, 0),
    row("not",  0x14,   Op::R_R_R__OR__R_R_IMM, 0),
    row("xor",  0x15,   Op::R_R_R__OR__R_R_IMM, 0),
    row("shl",  0x16,   Op::R_R_R__OR__R_R_IMM, 0),
    row("shr",  0x17,   Op::R_R_R__OR__R_R_IMM, 0),
    row("sar",  0x18,   Op::R_R_R__OR__R_R_IMM, 0),

    // Comparisons
    // Set condition codes based on variant used.
    row("cmp",  0x20,   Op::R_R__OR__R_I,   0),
    row("cmp.s",0x21,   Op::R_R__OR__R_I,   0),

    // Branch and its combinations
    // We encode the variant into the 4 flag bits to reduce opcode bloat.
    row("b",    0x30,   Op::R__OR__I, BR_ALWAYS),
    row("beq",  0x30,   Op::R__OR__I, BR_EQ),
    row("bne",  0x30,   Op::R__OR__I, BR_NE),
    row("blt",  0x30,   Op::R__OR__I, BR_LT),
    row("bgt",  0x30,   Op::R__OR__I, BR_GT),
    row("ble",  0x30,   Op::R__OR__I, BR_LE),
    row("bge",  0x30,   Op::R__OR__I, BR_GE),
];